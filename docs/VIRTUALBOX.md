# AiOS in VirtualBox

`build/aios.iso` (built by `scripts/23-iso.sh`) boots in VirtualBox and other
UEFI virtual machines. Its boot menu has two entries:

- **Try AiOS** (the default after 8 seconds): the desktop runs from the ISO.
  Nothing is written to the VM's disk, and notes and settings are lost when
  the VM is switched off.
- **Install AiOS**: writes AiOS to the VM's hard disk. It lists the disks,
  asks you to type the disk's name (usually `sda`) and then `yes`, writes
  about 1.5 GB, and asks you to remove the ISO and press Enter.

## Creating the VM

1. **New** VM: name `AiOS`, ISO image `aios.iso`, type **Linux**, version
   **Other Linux (64-bit)**. Tick **Skip Unattended Installation** if it is
   offered.
2. Hardware: **2048 MB** memory or more, **2** CPUs, and tick **Enable EFI**.
   This setting is required, because the ISO boots only with UEFI.
3. Hard disk: create a new one of **8 GB** or more.
4. Before the first start, open **Settings**:
   - **Display**: graphics controller **VMSVGA**, video memory **64 MB** or
     more, with **3D acceleration off**.
   - **System > Motherboard**: boot order with Optical first, then Hard Disk.
   - **Network**: NAT (the default) works for the AI connection.
5. **Start** the VM, then pick **Install AiOS** with the arrow keys.
6. When it says "Done", use **Devices > Optical Drives > Remove disk from
   virtual drive**, then press Enter. AiOS boots from the disk, and on its
   first boot the data partition grows to fill the rest of the disk.

## Notes

- This was tested in QEMU set up like VirtualBox: an AHCI disk, the ISO on
  the CD drive, an e1000 network card, a PS/2 keyboard and a USB tablet
  (`tests/boot/iso_test.py`, evidence in `tests/evidence/ISO/`). The
  VirtualBox display driver (`vmwgfx` for VMSVGA) is built in, but QEMU
  cannot emulate VMSVGA, so the display is the one part not yet seen
  working in VirtualBox itself. If the screen stays black after the boot
  messages, set the graphics controller to **VBoxSVGA** (driver
  `vboxvideo`) and try again.
- The pointer is the USB tablet that VirtualBox provides, so it works without
  Guest Additions. Shared folders and clipboard sharing are not supported.
- Secure Boot must be off. VirtualBox's EFI has it off by default.
