# Project files (.stf)

A StokesForge project is saved in a single file with the extension `.stf`. This page says what the file holds, which versions can open it, and how StokesForge protects your work when it saves.

## What a project file contains

One file holds the whole project:

- **The folders and everything in them:** workbooks, sheets, graphs and notes, with their names and their order.
- **The data:** every column with its values, type, long name, units, comments and formula. Missing values stay missing, and numbers come back exactly as they were saved.
- **The graphs:** their layers, series, axes, legends, titles, texts, shapes, colors and themes.
- **The analyses:** the calculations you ran, with their settings and their results. The result columns are ordinary columns of the workbook, so opening a file does not recalculate them. Only columns with a formula are computed again.
- **Final corrections:** the record of a baseline or correction that replaced data, and the original data it replaced, so that **Restore Original Data** still works after you reopen the project.
- **Where the data came from:** for imported columns, the source file, the import template and the file metadata, so that **Refresh from Source** works after you reopen the project.
- **The look of your workspace:** position, size, open or closed state and stacking order of the windows, and the width of the columns you resized.
- **Colors saved in the project** (the choice **Saved in project** in the color picker).

The file does **not** contain:

- **The undo history.** A project you reopen starts with an empty history. Save only when you are sure of your changes.
- **The zoom of a graph.** It is part of the view, not of the project. Use **Reset Zoom** or the zoom you set before an export.
- **Your preferences** (language, colors saved for all projects, export settings, toolbar layout). They belong to the application and are shared by all projects.
- **Detached windows.** A window you detached is saved as an ordinary open window.

## Save and open

| Action | Menu | Shortcut |
|---|---|---|
| Open a project | **File ▸ Open…** | <kbd>Ctrl</kbd>+<kbd>O</kbd> |
| Save | **File ▸ Save** | <kbd>Ctrl</kbd>+<kbd>S</kbd> |
| Save under another name | **File ▸ Save As…** | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> |

On macOS, use <kbd>Cmd</kbd> instead of <kbd>Ctrl</kbd>.

The first **Save** of a new project asks for a file name. The following ones use the same file. A dot after the project name in the title bar means unsaved changes.

If you start a new project, open another one or quit with unsaved changes, StokesForge asks whether to save first. **Discard Changes** loses them for good.

### Open from the command line

Give the path of a project after the program name:

```sh
stokesforge-app experiment-12.stf
```

An existing file opens at once. A path that does not exist opens an empty project, and **Save** then creates that file.

Without a path, StokesForge starts with an empty project. The very first time you start it, it opens a demonstration project instead, saved as `demo.stf` in the configuration folder of the program. You can find it again through **File ▸ Open…**.

## Compatibility between versions

StokesForge keeps a promise from version 1: **a project file saved by an older version always opens in a newer version.** The new version reads the old file as it is, and saves it in its own format. You do not convert anything by hand.

The promise goes one way. When you save a project with a newer version, an older version of StokesForge may not be able to read it.

### A file from a newer version

If a file was saved by a version of StokesForge newer than yours, StokesForge refuses to open it. The message gives the path, the format of the file and the newest format your version reads, and tells you to update StokesForge. Nothing is opened half-way and your current project stays as it was.

To open the file, install a newer version of StokesForge.

### Parts that cannot be read

Sometimes a file is only slightly newer than your version, or one part of it is unreadable. Then StokesForge keeps what it can:

- A part it does not know (for example a kind of column or plot added later) is set aside. The rest of the project opens, and the part is kept in the file when you save again.
- A calculation or a correction record that cannot be read is left out. The status bar says how many were left out, for example "2 calculations were discarded from the project".

If something was left out, saving over the same file asks first, in the window **Replace File**: "Saving replaces it without them: discarded data will be lost." Choose **Replace** to continue, or **Cancel** and use **File ▸ Save As…** to keep the original file untouched. Saving under another name never asks.

Other messages you may meet when opening a file:

| Message | What to do |
|---|---|
| "is not a StokesForge file" | The file is something else, or empty. |
| "is damaged" | The file is cut or corrupt. Look for a copy or a recovery copy. |
| "uses features from a newer version of StokesForge" | Update StokesForge. |
| "is too large to open" or "declares too many cells" | Raise the limit, see [File size](#file-size). |

## How saving protects your file

- **A save never leaves a half-written file.** StokesForge writes to a temporary file in the same folder and replaces your file only when the write is complete. If the save fails, your previous file is intact and your changes stay open in the program. The alert **Project Not Saved** offers **Save As…** to try another place.
- **StokesForge does not overwrite a file it cannot read.** If the target is a StokesForge file that is damaged, the save is refused with a message. Use **Save As…** with another name, or move that file first.
- **If the file changed elsewhere** since you opened or saved it, StokesForge refuses to save and tells you to reload it first. This protects the work of someone else who edited the same file.

## Recovery copies

While you work, StokesForge writes a recovery copy of the open project every few minutes. It is **never** written over your own file. It goes to a recovery folder, only when the project changed since the last copy.

If StokesForge closes in a way that did not let it save (a crash, a power cut), the window **Recover Projects** appears the next time you start it. Each row is a project, with the date of the copy. For each one you can choose:

| Button | Result |
|---|---|
| **Open** | Opens the recovery copy as unsaved work. Save the project to keep it. The list stays open for the other projects. |
| **Ignore** | Keeps the copy. It is offered again the next time you start. |
| **Delete** | Deletes the copy, after a confirmation. This cannot be undone. |

A recovery copy is deleted when you save the project, and when you close it normally. A copy may miss your very last changes.

Recovery copies are on by default, every 5 minutes. Change this in **File ▸ Preferences…**, page **General**, section **Files**: tick or untick **Save a recovery copy automatically**, and set **Copy every (min)**.

If the engine of the project stops unexpectedly, StokesForge shows an alert, writes a last recovery copy, and closes.

## File size

The `.stf` file is not compressed. Its size is about the size of the data: a number takes 8 bytes, so a column of one million values weighs about 8 MB. Text and graphs add little.

To protect your computer from a file received from someone else, StokesForge refuses very large files **before** reading them. In **File ▸ Preferences…**, page **General**, section **Files**, you can change:

- **Maximum file size (MB):** 512 MB by default, from 16 MB to 16 GB.
- **Maximum cells (millions):** the most cells a file may declare, 1,000 million by default.

If a message says a file is too large or declares too many cells, raise the limit and open it again. Saving a project larger than the limit still works; the status bar then warns you that you must raise the limit to reopen it.

Large projects also use memory when open: the whole project is loaded in memory.

## Keep your projects safe

- Save often. The undo history is not saved with the project.
- Back up the `.stf` file as you would any document. It is a single file, so copying or e-mailing it carries the whole project, data included.
- Before you save over a file that reported discarded parts, keep a copy of the original.
