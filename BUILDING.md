# Building and flashing

## Prerequisites

- **Rust** via [rustup](https://rustup.rs). The toolchain, ARM target and `llvm-tools` component are pinned in `fw/rust-toolchain.toml` and install automatically on the first build.
- **cargo-binutils**, to convert the ELF to a raw binary:
  ```sh
  cargo install cargo-binutils
  ```
- **dfu-util** 0.9 or newer:
  ```sh
  brew install dfu-util          # macOS
  sudo apt install dfu-util      # Debian / Ubuntu
  ```
  On Windows, use the binaries from [dfu-util.sourceforge.net](https://dfu-util.sourceforge.net) and install the WinUSB driver for the DFU device with [Zadig](https://zadig.akeo.ie).

## Build

```sh
cd fw
cargo build --release
cargo objcopy --release -- -O binary temtop-fw.bin
```

This produces `fw/temtop-fw.bin`, linked to run at 0x0800 5000, directly after the bootloader.

## Enter DFU mode

1. Switch the monitor off (hold POWER for 3 seconds).
2. Connect it to your computer with a USB data cable.
3. Hold **MENU** and press **POWER**. Keep holding MENU until the unit shows up as a DFU device.

Check that it is there:

```sh
dfu-util -l
```

You should see `[0483:df11]` with an alternate setting named `@Internal Flash`.

On Linux, either run `dfu-util` with `sudo` or add a udev rule:

```sh
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="df11", MODE="0666"' \
  | sudo tee /etc/udev/rules.d/50-stm32-dfu.rules
sudo udevadm control --reload-rules
```

## Back up the original firmware

Do this once, before flashing anything. It reads the complete 512 KB flash, including your unit's factory calibration:

```sh
dfu-util -a 0 -s 0x08000000:524288 -U m2000-backup.bin
```

Keep `m2000-backup.bin` somewhere safe. It is specific to your unit.

## Flash

With the unit in DFU mode:

```sh
dfu-util -a 0 -s 0x08005000:leave -D temtop-fw.bin
```

After flashing, the monitor switches itself off. Press **POWER** to start the new firmware.

Flashing only erases the pages the image occupies, so the bootloader (first 20 KB) and the factory calibration (last 2 KB page) are left as they are.

## Restore the original firmware

Strip the 20 KB bootloader from your backup and write the remainder back to the application area:

```sh
tail -c +20481 m2000-backup.bin > m2000-app.bin
dfu-util -a 0 -s 0x08005000:leave -D m2000-app.bin
```

This also restores the calibration page exactly as it was.

## Regenerating fonts (optional)

`fw/src/fonts.rs` is generated and checked in. To regenerate it, put the TTF files listed at the top of `fw/tools/gen_fonts.py` into a directory, then run:

```sh
pip install pillow
python3 fw/tools/gen_fonts.py fw/src/fonts.rs --font-dir path/to/fonts
```

## Troubleshooting

| Symptom | Fix |
|---|---|
| `dfu-util -l` shows nothing | Use a data cable, not a charge-only one. Make sure MENU is held while powering on. |
| `LIBUSB_ERROR_ACCESS` | Use `sudo` or add the udev rule above. |
| "Device's firmware is corrupt" printed before the download | Normal for this bootloader; dfu-util clears the status and continues. |
| Screen stays dark after flashing | The unit powers off after DFU. Press POWER. |
