#!/usr/bin/env sh
set -eu

if [ "$#" -ne 0 ]; then
  printf 'Usage: %s\n' "$0" >&2
  exit 2
fi

if [ "$(uname -s)" != Linux ]; then
  printf 'This script requires Linux.\n' >&2
  exit 1
fi

modules=$(lsmod)
loaded_modules=$(printf '%s\n' "$modules" | awk '$1 == "peak_usb" || $1 == "pcan" { print $1 }')
if [ -z "$loaded_modules" ]; then
  printf 'Neither peak_usb nor pcan is loaded; nothing to unload or restore.\n'
  exit 0
fi

# Keep the entire session privileged so restoring the drivers does not depend
# on a sudo credential timeout while testing in another terminal.
if [ "$(id -u)" -ne 0 ]; then
  exec sudo -- "$0"
fi

restore_modules=
cleanup() {
  status=$?
  trap - EXIT INT TERM HUP
  for module in $restore_modules; do
    printf 'Restoring %s...\n' "$module"
    if ! modprobe "$module"; then
      printf 'Could not restore %s. Run: sudo modprobe %s\n' "$module" "$module" >&2
      status=1
    fi
  done
  exit "$status"
}

# Always restore on Enter, EOF, ordinary errors, or a catchable interruption.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

for module in $loaded_modules; do
  # Record before unloading so an interruption immediately after modprobe also
  # restores it. Reloading a module whose unload failed is harmless.
  restore_modules="$module $restore_modules"
  printf 'Unloading %s...\n' "$module"
  modprobe -r "$module"
done

printf 'Test raw USB in another terminal. Close the test before restoring drivers.\n'
printf 'Press Enter to restore the PEAK driver(s)... '
IFS= read -r reply || true
