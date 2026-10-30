"""Transport-independent Scapy UDS responses, fault scenarios, and timing."""

import time

from scapy.all import Raw
from scapy.contrib.automotive.ecu import EcuAnsweringMachine
from scapy.contrib.automotive.uds import UDS
from scapy.plist import PacketList

VIN = b"WVWZZZ1JZXW000001"

SUPPRESSION_CASES = (
    "silent", "positive", "pending-positive", "pending-negative",
    "pending-only", "repeated-pending", "malformed-final", "bad-echo",
    "wrong-pending-sid", "extra-pending-data", "reserved-nrc",
)


def ecu_responses():
    from scapy.contrib.automotive import uds
    from scapy.contrib.automotive.ecu import EcuResponse

    memory_address = 0x123456
    memory = bytearray(b"abcdefgh")
    did_record = bytearray(b"deadbeef")
    transfer = {}

    def read_did(response, request):
        if not response.answers(request):
            return False
        response[Raw].load = bytes(did_record)
        return True

    def write_did(response, request):
        if not response.answers(request):
            return False
        did_record[:] = bytes(request[uds.UDS_WDBI].payload)
        return True

    def memory_region(packet):
        address = getattr(packet, f"memoryAddress{packet.memoryAddressLen}", -1)
        size = getattr(packet, f"memorySize{packet.memorySizeLen}", 0)
        offset = address - memory_address
        if offset < 0 or size <= 0 or offset + size > len(memory):
            return None
        return offset, size

    def read_memory(response, request):
        if not response.answers(request):
            return False
        region = memory_region(request[uds.UDS_RMBA])
        if region is None:
            return False
        offset, size = region
        response[uds.UDS_RMBAPR].dataRecord = bytes(memory[offset:offset + size])
        return True

    def write_memory(response, request):
        if not response.answers(request):
            return False
        packet = request[uds.UDS_WMBA]
        region = memory_region(packet)
        if region is None or len(packet.dataRecord) != region[1]:
            return False
        offset, size = region
        memory[offset:offset + size] = packet.dataRecord
        response[uds.UDS_WMBAPR].memoryAddress3 = packet.memoryAddress3
        response[uds.UDS_WMBAPR].memorySize2 = packet.memorySize2
        return True

    def begin_transfer(response, request):
        if not response.answers(request):
            return False
        packet = request.payload
        region = memory_region(packet)
        if region is None or packet.dataFormatIdentifier != 0:
            return False
        transfer.update(region=region, upload=uds.UDS_RU in request)
        return True

    def transfer_data(response, request):
        if not response.answers(request) or not transfer:
            return False
        offset, size = transfer["region"]
        data = request[uds.UDS_TD].transferRequestParameterRecord
        if transfer["upload"]:
            if data:
                return False
            record = bytes(memory[offset:offset + size])
        else:
            if len(data) != size:
                return False
            memory[offset:offset + size] = data
            record = b""
        response[uds.UDS_TDPR].transferResponseParameterRecord = record
        return True

    def transfer_exit(response, request):
        if not response.answers(request) or not transfer:
            return False
        packet = request.getlayer(uds.UDS_RTE)
        response[uds.UDS_RTEPR].transferResponseParameterRecord = (
            packet.transferRequestParameterRecord if packet is not None else b""
        )
        transfer.clear()
        return True

    return [
        EcuResponse(responses=uds.UDS() / uds.UDS_TPPR()),
        EcuResponse(responses=uds.UDS() / uds.UDS_DSCPR(
            diagnosticSessionType=3, sessionParameterRecord=b"\x03\xe8\x01\xf4")),
        EcuResponse(responses=uds.UDS() / uds.UDS_NR(
            requestServiceId=0x10, negativeResponseCode=0x33)),
        EcuResponse(responses=uds.UDS() / uds.UDS_ERPR(resetType=1)),
        EcuResponse(responses=uds.UDS() / uds.UDS_ERPR(resetType=4, powerDownTime=2)),
        EcuResponse(responses=uds.UDS() / uds.UDS_SAPR(
            securityAccessType=1, securitySeed=b"seed")),
        EcuResponse(responses=uds.UDS() / uds.UDS_SAPR(securityAccessType=2),
            answers=lambda response, request: response.answers(request)
            and request[uds.UDS_SA].securityKey == b"key!"),
        EcuResponse(responses=uds.UDS() / uds.UDS_NR(
            requestServiceId=0x27, negativeResponseCode=0x35)),
        EcuResponse(responses=uds.UDS() / uds.UDS_RDBIPR(dataIdentifier=0x1234)
            / Raw(bytes(did_record)), answers=read_did),
        EcuResponse(responses=uds.UDS() / uds.UDS_RDBIPR(dataIdentifier=0xf190)
            / Raw(VIN)),
        EcuResponse(responses=uds.UDS() / uds.UDS_WDBIPR(dataIdentifier=0x1234),
            answers=write_did),
        EcuResponse(responses=uds.UDS() / uds.UDS_RMBAPR(), answers=read_memory),
        EcuResponse(responses=uds.UDS() / uds.UDS_WMBAPR(
            memoryAddressLen=3, memorySizeLen=2), answers=write_memory),
        EcuResponse(responses=uds.UDS() / uds.UDS_RDTCIPR(
            reportType=1, DTCStatusAvailabilityMask=0xff,
            DTCFormatIdentifier=1, DTCCount=2)),
        EcuResponse(responses=[
            uds.UDS() / uds.UDS_NR(requestServiceId=0x31, negativeResponseCode=0x78),
            uds.UDS() / uds.UDS_RCPR(routineControlType=1, routineIdentifier=0x1234)
            / Raw(b"\x00"),
        ]),
        EcuResponse(responses=uds.UDS() / uds.UDS_RCPR(
            routineControlType=2, routineIdentifier=0x1234)),
        EcuResponse(responses=uds.UDS() / uds.UDS_RCPR(
            routineControlType=3, routineIdentifier=0x1234) / Raw(b"done")),
        EcuResponse(responses=uds.UDS() / uds.UDS_RDPR(
            memorySizeLen=2, maxNumberOfBlockLength=b"\x00\x0a"), answers=begin_transfer),
        EcuResponse(responses=uds.UDS() / uds.UDS_RUPR(
            memorySizeLen=2, maxNumberOfBlockLength=b"\x00\x0a"), answers=begin_transfer),
        EcuResponse(responses=uds.UDS() / uds.UDS_TDPR(blockSequenceCounter=1),
            answers=transfer_data),
        EcuResponse(responses=uds.UDS() / uds.UDS_RTEPR(), answers=transfer_exit),
    ]


class SuppressionAwareECU(EcuAnsweringMachine):
    """The same UDS fault scenarios and response timing on every transport."""

    def parse_options(self, suppression_case=None, **kwargs):
        self.suppression_case = suppression_case
        super().parse_options(**kwargs)

    def make_reply(self, request):
        data = bytes(request)
        case = self.suppression_case
        if case == "silent":
            return PacketList()
        suppress = (len(data) >= 2 and data[0] in
                    {0x10, 0x11, 0x19, 0x27, 0x31, 0x3e} and data[1] & 0x80)
        if not suppress:
            return super().make_reply(request)

        # Match the actual sub-function and echo only its lower seven bits.
        normalized = UDS(data[:1] + bytes([data[1] & 0x7f]) + data[2:])
        replies = super().make_reply(normalized)
        pending = b"\x7f" + data[:1] + b"\x78"
        if case == "positive":
            return replies  # Deliberately noncompliant: ignore suppression.
        if case == "pending-positive":
            return PacketList([UDS(pending), *replies])
        if case == "pending-negative":
            return PacketList([UDS(pending), UDS(b"\x7f" + data[:1] + b"\x33")])
        if case == "pending-only":
            return PacketList([UDS(pending)])
        if case == "repeated-pending":
            return PacketList([UDS(pending) for _ in range(3)] + list(replies))
        if case == "wrong-pending-sid":
            return PacketList([UDS(bytes([0x7f, data[0] ^ 1, 0x78]))])
        if case == "extra-pending-data":
            return PacketList([UDS(pending + b"\xaa")])
        if case == "reserved-nrc":
            return PacketList([UDS(b"\x7f" + data[:1] + b"\x15")])
        if case in {"malformed-final", "bad-echo"}:
            final = bytes([data[0] | 0x40, data[1] & 0x7f])
            if case == "bad-echo":
                final = bytes([data[0] | 0x40, data[1]])
            elif data[0] == 0x3e:
                final += b"\xaa"  # TesterPresent has no response data.
            return PacketList([UDS(pending), UDS(final)])

        # NRC 0x78 requires a final reply even with suppression requested.
        if any(bytes(reply) == pending for reply in replies):
            return replies
        return PacketList([reply for reply in replies if bytes(reply)[0] == 0x7f])

    def send_reply(self, replies, send_function=None):
        # Deterministic spacing lets tests exercise repeated P2* reloads.
        send = send_function or self._main_socket.send
        for index, reply in enumerate(replies):
            if index:
                time.sleep(0.075)
            send(reply)
