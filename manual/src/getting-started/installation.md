# Installation

This page tells you what you need to run StokesForge, where to download it, and how to install it on Windows and on Linux. It also explains how to update to a new version and where to find the manual that matches your version.

## What you need

| | Windows | Linux |
|---|---|---|
| System | 64-bit Windows | 64-bit Linux on an x86-64 processor, with a graphical desktop |
| Display server | The Windows desktop | Wayland or X11 |
| Graphics | A graphics driver with DirectX 12 or Vulkan | A graphics driver with Vulkan |
| Python | Included | Included |

There is no package for macOS.

StokesForge draws its windows with the graphics card, through the `wgpu` library. Any recent graphics driver is enough. You do not need a powerful card.

On Linux, Vulkan comes with the driver of your graphics card (Mesa for Intel and AMD, the proprietary driver for NVIDIA). A computer without a graphics card, or a virtual machine, can use the software driver of Mesa. On Debian and Ubuntu it is the package `mesa-vulkan-drivers`.

Python is included in the package. You do not need to install Python, numpy or scipy, and a Python that is already on your computer is not used. See [The script console](../scripting/console.md).

StokesForge keeps the whole project in memory while it is open. A project with very large tables needs enough free memory for its data. See [File size](../reference/project-files.md#file-size).

## Download

Packages are published with each version on the
[releases page](https://gitlab.com/askrzypczak/stokesforge/-/releases). Open the release you want. Its assets are:

| Asset | File name | For |
|---|---|---|
| Windows installer | `StokesForge-<version>-windows-setup.exe` | Windows |
| Linux x86_64 archive | `StokesForge-<version>-linux-x86_64.tar.gz` | Linux |
| User manual | `StokesForge-<version>-manual.tar.gz` | This manual, for the same version |

`<version>` is the version number, for example `1.0.0`.

## Install on Windows

1. Download `StokesForge-<version>-windows-setup.exe`.
2. Double-click the file to start the installer.
3. Windows may show the message "Windows protected your PC" and name the publisher as unknown. The installer is not signed yet. Click **More info**, then **Run anyway**.
4. Follow the installer. By default it installs for your user only and asks for no administrator rights. At its start, it offers to install for all users of the computer instead; Windows then asks for an administrator password.
5. At the end, leave **Launch StokesForge** ticked to start the application, or untick it.

The installer puts StokesForge in a folder named `StokesForge`:

- For your user only: `%LOCALAPPDATA%\Programs\StokesForge`.
- For all users: `C:\Program Files\StokesForge`.

It adds two entries to the Start menu: **StokesForge** and **Uninstall StokesForge**. It does not add an icon to the desktop.

### Open `.stf` files

The installer makes StokesForge appear in the **Open with** list for `.stf` files. It does not take over the files if another program is already their default.

To make StokesForge the default for your projects, right-click a `.stf` file, choose **Open with**, then **Choose another app**, pick StokesForge and tick **Always use this app**.

### Uninstall on Windows

Open the Windows **Settings**, go to **Apps**, pick StokesForge and choose **Uninstall**. You can also use **Uninstall StokesForge** in the Start menu.

Uninstalling removes the program. It does not remove your projects.

## Install on Linux

The Linux package is an archive with an installer script inside. There is no package for your package manager.

1. Download `StokesForge-<version>-linux-x86_64.tar.gz`.
2. Open a terminal, unpack the archive and run the installer:

   ```sh
   tar -xzf StokesForge-<version>-linux-x86_64.tar.gz
   cd StokesForge-<version>
   ./install.sh
   ```

3. Start the application from the application menu (**StokesForge**, in the Science category), or from a terminal:

   ```sh
   stokesforge
   ```

By default the installer works for your user only and asks for no administrator rights. It puts:

- the program in `~/.local/share/stokesforge/<version>/` (or under `$XDG_DATA_HOME` if you set that variable), with a link named `current` to the installed version;
- the command `stokesforge` in `~/.local/bin`. If that folder is not in your `PATH`, the installer says so; the menu entry works anyway;
- the menu entry **StokesForge**, and the file type of `.stf` projects, under `~/.local/share`.

You can delete the unpacked folder once the installer has finished: the program has been copied.

The folder of the program holds `stokesforge-app`, the Python it uses (in `lib/python`), the notices of the licences of the software it contains, and a check program, `stokesforge-selftest`. Keep these files together.

The archive is built on a recent Ubuntu. A much older distribution may fail to start the program because its system libraries are too old.

### Open `.stf` files

The installer makes StokesForge appear in the **Open with** list for `.stf` files. It does not take over the files if another program is already their default. To make it the default, right-click a `.stf` file in your file manager, choose **Open with**, pick StokesForge and choose to always use it.

### Install for all users

To install StokesForge for every user of the computer, run the installer with `sudo`:

```sh
sudo ./install.sh --system
```

The program goes to `/opt/stokesforge/<version>/`, the command to `/usr/local/bin/stokesforge`, and the menu entry to `/usr/local/share`. The installer refuses to run if a package of your distribution already owns `/opt/stokesforge`. It never starts `sudo` by itself.

### Wayland and X11

StokesForge uses Wayland when your session offers it, and X11 otherwise. If the window does not behave well on Wayland, start the program on X11 with the variable `STOKESFORGE_X11`:

```sh
STOKESFORGE_X11=1 stokesforge
```

See [Troubleshooting](../reference/troubleshooting.md) for other start-up problems.

### Uninstall on Linux

Run the installer again with `--uninstall`. The installed copy can do it, from any folder:

```sh
~/.local/share/stokesforge/current/install.sh --uninstall
```

This removes the program, the command, the menu entry and the file type. For an installation for all users, use `sudo /opt/stokesforge/current/install.sh --system --uninstall`.

Uninstalling never removes your projects, your preferences (`~/.config/stokesforge`) or the recovery copies. The recovery copies are in a `recovery` folder inside `~/.local/share/stokesforge/`; the installer leaves that folder in place.

To remove one kept version only (see below), give its number:

```sh
~/.local/share/stokesforge/current/install.sh --uninstall 1.0.0
```

The current version cannot be removed this way: use `--uninstall` alone.

## First start

The first time you start StokesForge, it opens a demonstration project. It is saved as `demo.stf` in the configuration folder, so you can open it again with **File ▸ Open…**. Later starts open an empty project.

To start a project of your own, choose **File ▸ New ▸ New Project**. The walk-through [Your first project](first-project.md) starts there.

To open a project directly, give its path after the program name:

```sh
stokesforge experiment-12.stf
```

On Linux the command is `stokesforge`. On Windows, the program is `stokesforge-app.exe` in the installation folder. See [Open from the command line](../reference/project-files.md#open-from-the-command-line).

The interface is in English at first. To change this, see [Change the interface language](../guide/interface.md#change-the-interface-language).

### Where your settings are kept

StokesForge keeps your preferences, your saved import templates, your saved colors and the demonstration project in one configuration folder. The recovery copies are in a `recovery` folder next to the data of the application.

| System | Configuration folder |
|---|---|
| Linux | `~/.config/stokesforge` |
| Windows | `%APPDATA%\StokesForge\config` |

To keep all of this in a folder of your choice, for example on a USB drive, set the variable `STOKESFORGE_CONFIG_DIR` to that folder before you start the program. The recovery copies are then in its `recovery` subfolder.

## Update to a new version

1. Read the notes of the new version on the [releases page](https://gitlab.com/askrzypczak/stokesforge/-/releases).
2. Save your projects, then close StokesForge.
3. On Windows, run the new installer. It replaces the old version.
   On Linux, unpack the new archive and run its `./install.sh`. It installs the new version, makes `stokesforge` and the menu entry open it, and removes the version it replaces.

Your preferences are kept.

### Keep the previous version on Linux

Some versions change the Python interface of the scripts, and a script written for the old version may stop working. To keep the old version beside the new one, update with `--keep-previous`:

```sh
./install.sh --keep-previous
```

The old version keeps its folder. It gets its own command, `stokesforge-<old version>`, and its own menu entry, **StokesForge <old version>**. `stokesforge` and the entry **StokesForge** open the newest version. A later update without `--keep-previous` removes only the version that was current; the versions you kept stay until you remove them with `--uninstall <version>`.

Two limits apply to kept versions:

- All versions share the same preferences (`~/.config/stokesforge`) and the same recovery copies.
- A version may refuse a project saved by a newer one (see below). Do not save an important project with the new version if you still need to open it with the old one.

If you run the installer of a version older than the newest one installed, it says so and asks for confirmation. If you accept, it installs the older version beside the newest one, in the same way as a kept version, and `stokesforge` still opens the newest. Without a terminal, add `--yes` to answer yes in advance.

A project saved by an older version always opens in a newer version. The reverse is not true: a project you saved with a new version may be refused by an older one. Keep a copy of important projects before you update. See [Compatibility between versions](../reference/project-files.md#compatibility-between-versions).

## The manual of your version

A manual is published for each version. The one that matches yours is attached to its release, under the name `StokesForge-<version>-manual.tar.gz`. Unpack it and open `index.html` in a web browser. Final versions are also published on the web. The title of the manual shows the version it describes.

Features change from one version to the next. If a button or a menu of this manual is not in your copy of StokesForge, check that the manual and the program have the same version.

## Command-line tool

The packages contain the graphical application only. They do not contain the command-line program `stokesforge-cli`. To run Python scripts, use the script console of the application. See [The script console](../scripting/console.md).
