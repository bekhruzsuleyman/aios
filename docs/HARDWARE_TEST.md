# Testing AiOS 0.1 on a real computer

This is the one step the build could not do itself: booting on physical
hardware. Everything below is for you, the human. It takes about 15 minutes.

## What you need
- A USB stick of **8 GB or more** (everything on it will be erased).
- A 64-bit x86 PC or laptop with **UEFI** firmware (almost every machine from
  2012 on). Apple Macs and ARM machines are not supported in 0.1.
- The file `build/aios-usb.img` from this repository (about 3.5 GB).

## 1. Write the stick
On a Linux machine, plug in the stick, then find its device name:

    lsblk -o NAME,MODEL,SIZE,TRAN,RM

The stick is the line with `TRAN` = `usb` and the right size, for example
`sdb`. Then:

    sudo scripts/write-usb.sh /dev/sdb

The script refuses anything that is not a whole removable/USB disk, anything
mounted, and the disk your system runs from. It shows what it will erase and
asks you to type the device path again before writing.

On Windows or macOS you can use a raw image writer instead (Rufus in "DD
image" mode, balenaEtcher, or `dd` on macOS). Write the image to the whole
device, not to a partition.

## 2. Boot it
1. Plug the stick into the target machine and power it on.
2. Open the boot menu (usually **F12**, **F11**, **F9**, **Esc** or **F8**
   right after power-on; on Lenovo laptops F12, Dell F12, HP F9 or Esc,
   ASUS F8 or Esc).
3. Choose the USB stick's **UEFI** entry.
4. If the machine refuses to boot it, enter the firmware setup (usually
   **F2** or **Del**) and **disable Secure Boot**. AiOS 0.1 is not signed.
   You can turn Secure Boot back on afterwards.

## 3. What you should see
- No boot menu (it boots straight in) and, within about 5 to 20 seconds, a
  pastel desktop with a white bar at the top: `AI · NATIVE OS`,
  `ASK SUPER+SPACE`, a status chip, the clock and the date.
- The status chip says **AI NOT CONNECTED** until you connect an AI.
- Press **Super+Space** (the Windows key and Space): the palette opens.
  Try `note: hello from real hardware`, `open terminal` (it asks first; press
  Alt+A), or `search linux`.
- Wi-Fi: click the **WI-FI** chip in the top bar to pick a network and type
  its password. Wired Ethernet connects by itself.
- To connect an AI, press **Alt+S** in the palette (or click the status chip)
  and enter an OpenAI-compatible endpoint; see README.md.
- On first boot the data partition grows to fill the stick. Your notes and
  settings live there and survive reboots; everything else is read-only.

## 4. If it does not work
Please note the machine model and what you saw (a photo of the screen helps).

**It does not appear in the boot menu:** the machine may be in legacy/CSM
mode. Enable UEFI boot in the firmware setup.

**It stops with a black screen or text:** wait two minutes, then power off.
AiOS keeps its system log on the stick's data partition, so the log of the
failed boot can be read on any Linux machine:

    lsblk -o NAME,LABEL           # find the partition labelled aios-data, e.g. sdb3
    sudo mount -o ro /dev/sdb3 /mnt
    sudo journalctl -D /mnt/journal -b -1 > aios-journal.txt   # or -b 0 for the only boot
    sudo journalctl -D /mnt/journal -k > aios-dmesg.txt          # kernel messages
    sudo umount /mnt

**It boots but something is wrong:** same as above; the log covers the whole
session.

**The data partition is missing from the log:** the kernel may not have
found the stick's controller. Boot a normal Linux live USB on the same
machine and send `lspci -nn`, `lsusb` and `dmesg` from it.

**Wi-Fi does not show networks:** the adapter may need firmware that is not
in the image. `lspci -nn | grep -i net` on a live USB tells us which.

Send the results back with the model name; the next build will add what is
missing.
