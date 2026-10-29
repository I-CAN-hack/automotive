"""Scapy process lifecycle and DoIP endpoint setup shared by protocol tests."""

from __future__ import annotations

import asyncio
import sys
import tempfile
from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path

from automotive.transports.doip import DiscoveryConfig

ROOT = Path(__file__).resolve().parents[2]


@asynccontextmanager
async def scapy_process(args: list[str]) -> AsyncGenerator[asyncio.subprocess.Process]:
    """Capture ECU errors and always terminate/reap the subprocess."""
    with tempfile.TemporaryFile() as stderr:
        process = await asyncio.create_subprocess_exec(
            sys.executable, *args, stdout=asyncio.subprocess.PIPE, stderr=stderr, cwd=ROOT
        )
        try:
            yield process
        except BaseException:
            stderr.seek(0)
            output = stderr.read().decode(errors="replace")
            if output:
                print("Scapy ECU stderr:\n" + output, file=sys.stderr)
            raise
        finally:
            if process.returncode is None:
                process.terminate()
            try:
                await asyncio.wait_for(process.wait(), 3)
            except TimeoutError:
                process.kill()
                await process.wait()


@dataclass
class DoipECU:
    host: str
    port: int

    @property
    def discovery(self) -> DiscoveryConfig:
        host = f"[{self.host}]" if ":" in self.host else self.host
        return DiscoveryConfig(bind_address=f"{host}:0", destination=f"{host}:{self.port}", timeout=0.1)


@asynccontextmanager
async def doip_ecu(
    host: str = "127.0.0.1",
    case: str = "normal",
    announce_to: int | None = None,
    *,
    uds_case: str | None = None,
) -> AsyncGenerator[DoipECU]:
    args = [str(ROOT / "scripts/vecu_doip.py"), "--host", host, "--case", case]
    if announce_to is not None:
        args += ["--announce-to", str(announce_to)]
    if uds_case is not None:
        args += ["--suppression-case", uds_case]
    async with scapy_process(args) as process:
        assert process.stdout is not None
        ready = (await asyncio.wait_for(process.stdout.readline(), 10)).decode().strip()
        assert ready.startswith("READY "), "Scapy ECU did not start"
        yield DoipECU(host, int(ready.removeprefix("READY ")))
