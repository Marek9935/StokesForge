# Troubleshooting

This page lists problems you may meet, with their cause and what to do. The problems are grouped by area: installation and start-up, import, graphs, analysis, files, scripting and copying figures. The last section tells you how to report a problem.

Messages of StokesForge are quoted as they appear. A part that changes (a file name, a number) is shown as `…`. The status bar keeps the last 50 messages: click it to read a message that has gone. See [Status bar](../guide/interface.md#status-bar).

## Installation and start-up

### Windows says "Windows protected your PC"

The installer is not signed yet, so Windows names the publisher as unknown. Click **More info**, then **Run anyway**. See [Install on Windows](../getting-started/installation.md#install-on-windows).

### The window stays blank, black or does not open

**Cause.** StokesForge draws with the graphics card. A missing, old or broken graphics driver, a virtual machine, or a remote desktop can prevent it.

**What to do.**

1. Update the driver of your graphics card, then restart the computer.
2. On Linux, check that a Vulkan driver is installed. On a computer without a graphics card or in a virtual machine, install the software driver of Mesa (on Debian and Ubuntu, the package `mesa-vulkan-drivers`).
3. Ask the program to use another graphics interface, with the variable `WGPU_BACKEND`. The values are `vulkan`, `dx12`, `metal`, `opengl` and `webgpu`. For example, on Linux:

   ```sh
   WGPU_BACKEND=opengl stokesforge
   ```

   On Windows, in PowerShell, start the program from its folder:

   ```powershell
   $env:WGPU_BACKEND = "vulkan"
   & "$env:LOCALAPPDATA\Programs\StokesForge\stokesforge-app.exe"
   ```

4. On a computer with two graphics cards, `WGPU_POWER_PREF=low` chooses the economical card and `WGPU_POWER_PREF=high` the powerful one.

If a value does not work, try another. If none works, report the problem (see [Report a problem](#report-a-problem)) and name your graphics card and driver.

### The window misbehaves on Linux with Wayland

**Cause.** StokesForge uses Wayland when your session offers it. Some compositors or drivers have problems.

**What to do.** Start the program on X11 with `STOKESFORGE_X11=1`:

```sh
STOKESFORGE_X11=1 stokesforge
```

On a Wayland session this runs it through XWayland.

### The program does not start on an old Linux distribution

**Cause.** The Linux archive is built on a recent Ubuntu. A much older distribution has older system libraries.

**What to do.** Use a more recent distribution, or report the problem.

### "Python was not found in …"

**Cause.** The folder `lib/python` is missing next to the program, or the program was moved alone.

**What to do.** Unpack the package again and keep its files together. On Windows, run the installer again. To check the packaged Python, run `stokesforge-selftest` from the Linux folder. It prints its checks and ends with code 0 when everything works. See [Install on Linux](../getting-started/installation.md#install-on-linux).

### The title of the Python Script panel says "— Not available in this build"

**Cause.** This copy of StokesForge was built without Python.

**What to do.** Install the packages from the releases page. They include Python. See [The script console](../scripting/console.md).

### The interface is not in the language I want

**Cause.** The language is a preference. The variable `STOKESFORGE_LANG` (`en` or `fr`), when it is set, wins over the preference.

**What to do.** Choose **File ▸ Language**. If nothing changes at the next start, remove the variable `STOKESFORGE_LANG` from your environment. See [Change the interface language](../guide/interface.md#change-the-interface-language).

### A double-click on a `.stf` file does not open StokesForge (Windows)

**Cause.** The installer adds StokesForge to the **Open with** list but does not make it the default program.

**What to do.** Right-click the file, choose **Open with**, then **Choose another app**, pick StokesForge and tick **Always use this app**.

## Import

For the full list of import problems, see [Common problems](../guide/import.md#common-problems). The most frequent ones are below.

### A number is read as text

**Cause.** The column becomes **Mixed** when some of its values are not numbers. The usual causes are a wrong **Decimal separator**, a wrong **Delimiter**, header lines read as data, or a word such as `n/a` for a missing value.

**What to do.** Open the wizard with **File ▸ Import ▸ Reopen Last Import…** and check, in this order: the **Decimal separator**, the **Delimiter**, the **Thousands separator**, the **Missing values**, the **Header**, and **Read As** in step 4. The preview shows the effect of each change.

### The decimal comma

**Cause.** The file writes `1,5` and the **Decimal separator** is **Dot**, which is the default of a new template.

**What to do.** Choose **Comma** or **Auto** for the **Decimal separator**. In a cell that you type yourself, a decimal comma is accepted: the status bar says `"1,5" was read as 1.5 (decimal comma)`.

### The column names are the first row of data, or the units are missing

**Cause.** The wizard does not know that the first line is a header.

**What to do.** In step 3, tick **Names in first line**, or set the **Header** and the **Subheader lines**, then give the roles **Long Name** and **Units** their line numbers. See [Step 3: header and data end](../guide/import.md#step-3-header-and-data-end).

### Messages of the Import Wizard

| Message | What it means and what to do |
|---|---|
| "The file is not readable text" | The file is binary. Export a text version from the instrument software. |
| "The file is empty" | The file has no content. Check that the export finished. |
| "No data found after the header. Check the number of header lines" | The header takes more lines than the file has. Change the header in step 3. |
| "A space cannot both separate the columns and group the thousands. Choose another delimiter or another thousands separator" | **Whitespace** as the delimiter and **Space** as the thousands separator cannot be used together. |
| "Choose a file before detecting the header" | **Detect Header** needs a file in the list of step 1. |
| "An import is already running" | Wait for the import to end, or click **Stop** in the status bar. |

## Graphs

### A formula in a title is not rendered

**Cause.** A title is read as Typst markup. Only the text between dollar signs is math. A formula with a mistake keeps your text as typed, and the field gets a red frame once you leave it.

**What to do.**

- Put the formula between dollar signs: `$lambda$`, not `lambda`.
- Put upright text in double quotes: `$"cm"^(-1)$`, not `$cm^-1$`.
- Put several characters of an exponent or an index in parentheses: `^(-1)`.
- Write `\$`, `\_`, `\*` and `\#` to show the character itself in plain text.
- Hover over the small help icon (<img class="icon" src="../images/icons/question.neutral.svg" alt="">) next to the field to see the common forms.

See [Titles and formulas](../guide/graphs.md#titles-and-formulas).

### A curve does not appear

Check these causes:

- The series is hidden. Tick its visibility box in the Object Manager.
- The axis has a **Log scale**. Zero and negative values are not drawn on it.
- You typed a range in **From** and **to** that does not contain the data. Click **Auto**.
- You zoomed in. Double-click an empty part of the plot to reset the view.
- The plot is empty because no Y column was selected: the status bar says "No Y column selected. Select a Y column to plot".

See [Axes](../guide/graphs.md#axes) and [Zoom and pan](../guide/graphs.md#zoom-and-pan).

### The graph does not follow my data

**Cause.** Several things are by design.

- A graph already plotted does not change when you move a column.
- A graph copied inside the same project uses the original data. A graph pasted into another project brings its own data, in a hidden workbook. Choose **Graph ▸ Show Data** to open it.
- The curve of an analysis turns gray, with a warning triangle, when its source data changed and it was not recalculated.

**What to do.** For a gray curve, use **Analysis ▸ Recalculate Out of Date**. Otherwise check which column the series plots in the Object Manager. If the figure stays wrong after these checks, report it with the steps to reproduce it.

### The exported figure shows the zoomed view

**Cause.** An export shows what you see, with the current zoom.

**What to do.** Click **Reset Zoom** under the figure, or choose **Graph ▸ Reset Zoom**, then export again. See [What the file contains](../guide/export.md#what-the-file-contains).

### Other graph messages

| Message | What to do |
|---|---|
| "There are already three overlaid layers in this graph" | A graph holds at most three overlay layers. |
| "Graph properties are open. Apply or cancel them before changing a detached window" | Click **OK** or **Cancel** in **Graph Properties**. |
| "Apply or cancel the graph properties before dropping files" | Close **Graph Properties** first. |

## Analysis

### The fit does not converge

**Cause.** The model does not reach the data from the starting values. You see the warning "The fit did not converge: the model does not reach the data, check the starting values", or the message "The fit did not converge after … iterations. Change the starting values or the model".

**What to do.**

1. Click **Auto** to fill the **Start** column with an estimate from the data.
2. Type starting values that are close to what you expect.
3. Set **Min** and **Max** bounds, or tick **Fixed** for a parameter you know.
4. Fit a smaller **Range** around the part that the model describes.
5. Try another model. A linear model is solved directly and needs no starting values.

The best attempt is still drawn. See [Fitting](../guide/analysis.md#fitting).

### The R² is low

**Cause.** The model does not describe the data well. A dialog titled **Low R²** says "Some results follow the data poorly (R² below 0.6). Check the model and the starting values." The status bar shows a badge such as **⚠ 3 low R²**.

**What to do.** Look at the residuals: tick **Write the residuals** and plot them. Check that the model has the right shape, that the range excludes other bands, and that the starting values are good. To stop the dialog, untick **Warn when R² is low** in the **Output** block or in **File ▸ Preferences…**. See [Goodness of fit](../guide/analysis.md#goodness-of-fit).

### Analysis messages

| Message | What to do |
|---|---|
| "Select at least one Y column first" | Select a column with the role **Y**, or a curve, then open the analysis. |
| "Every parameter is fixed. Free at least one parameter" | Untick **Fixed** for one parameter. |
| "The data do not determine the parameters. Fix a parameter or widen the range" | The model has too many parameters for the data. |
| "… parameter(s) are not determined by the data: no uncertainty is given" | The result is computed, but those parameters have no standard error. |
| "Only … usable points, at least … are needed. Check the range and the data" | Widen the range, or check for empty cells. |
| "Two points share the same X value. Remove or average the duplicates first" | Clean the X column. |
| "The baseline needs at least … anchor points, … given" | Place more anchor points. |
| "The background zones hold … points, … are needed for degree …. Widen the zones or lower the degree" | Widen the zones or lower the **Degree**. |
| "The error column holds a missing, zero or negative value. Fix it or turn weighting off" | Fix the error column, or untick **Weight by the error column**. |
| "The expression is empty", "Unknown function … at position …" | Fix the expression of the custom model. |
| "This column is produced by a calculation. Detach it from the calculation to edit its values" | Right-click the header and choose **Detach from Calculation**. |
| "This column is computed by a formula. Edit the formula to change its values" | Change the **F(x)=** cell, or use **Remove Formula, Keep Data**. |
| "Column … does not exist in this sheet" | A formula names a letter that is not in the sheet. |
| "Formulas refer to each other in a loop. Remove one of the references" | Break the loop between two formulas. |
| "Unknown function …. Check its spelling" | A name followed by `(` is not one of the functions listed in [Formula syntax](../guide/projects-and-workbooks.md#formula-syntax). |
| "Unknown function …. Did you mean …?" | The name is close to a function: write that one. |
| "Function … takes … arguments (found …)" | Give the function the number of values it takes, separated by commas (`pi()` takes none). |
| "Function … takes … arguments (found …). Use … instead" | Another function does what you meant: `logx(B, 2)` for a base, `atan2(y, x)` for two values, `root(x, n)` for the n-th root. |
| "… is a column, not a function. To multiply, write …*(...)" | Add the `*` sign between the column and the parenthesis: `A*(B/C)`. |
| "For the number π, write pi() (a lone pi would be column PI)" | Write `pi()` with parentheses. |
| "Function … reads a whole column. Give it a column letter, not a calculation (B, not B*2)" | The first value of `at`, `max(B)`, `mean(B)`, `integral(B)`… (and the third of `at(B, x, D)`) is a column letter. Calculate the expression in another column first. |
| "The column this formula reads has no X column. Put a column with the role X to its left" | `at`, `integral` and the range forms (`max(B, 330, 350)`) read the X paired with the column. Change the role of a column to X, or move it to the left of the one the formula reads. The cells show `---` until then. |
| "The formula found no point to compute from: the column or the range is empty, or the value asked for is outside the X data. Check the bounds" | Every cell of the result is empty. Check the bounds, the X values and the data of the column. |

See [Analysis](../guide/analysis.md) and [Columns computed by a formula](../guide/projects-and-workbooks.md#columns-computed-by-a-formula).

## Files

### StokesForge refuses a project from a newer version

**Message.** "… was saved by a newer version of StokesForge (file format …, this version reads up to …): update StokesForge to open it", or "… uses features from a newer version of StokesForge (…)".

**Cause.** The file was saved by a newer version than yours. Older files always open in newer versions, but not the reverse.

**What to do.** Install the newer version. See [Update to a new version](../getting-started/installation.md#update-to-a-new-version) and [A file from a newer version](project-files.md#a-file-from-a-newer-version).

### Other messages when opening or saving a project

| Message | What to do |
|---|---|
| "… is not a StokesForge file" | The file is something else, or empty. |
| "… is damaged: …" | The file is cut or corrupt. Look for a copy, or for a recovery copy. |
| "… is too large to open (… MB, limit … MB). Raise the limit in File > Preferences > General, then open it again" | Raise **Maximum file size (MB)** in **File ▸ Preferences…**. |
| "… declares too many cells (… million, limit … million). Raise the limit in File > Preferences > General, then open it again" | Raise **Maximum cells (millions)**. |
| "… was modified elsewhere since it was opened or saved. Reload it before saving" | Another program or person changed the file. Save under another name with **File ▸ Save As…**, or reopen the file. |
| "Cannot save over …: it is a StokesForge file that cannot be read (…). Use Save As with another name, or move that file first" | Use **File ▸ Save As…** with another name. |
| **Project Not Saved**: "The project could not be saved. Its changes are kept." | Click **Save As…** and choose another place. The disk may be full or read-only. |
| **Project Not Opened**: "The project could not be opened. The current project is unchanged." | Check the file, or click **Open Another…** to pick a file. |

The window **Replace File** appears when parts of a file were left out on opening. **Replace** loses them for good. Choose **Cancel** and use **File ▸ Save As…** to keep the original. See [Parts that cannot be read](project-files.md#parts-that-cannot-be-read).

### StokesForge crashed and I lost work

At the next start, the window **Recover Projects** lists the recovery copies that are newer than your saved files. Click **Open** to open a copy, then save it. **Ignore** keeps the copy for later; **Delete** removes it.

A recovery copy may miss your last changes. If the window does not appear, check that **Save a recovery copy automatically** is ticked in **File ▸ Preferences… ▸ General**. The copies are in the `recovery` folder next to the data of the application (see [Where your settings are kept](../getting-started/installation.md#where-your-settings-are-kept)).

If the project engine stops, the alert **Session Stopped** says "The project engine stopped unexpectedly. Changes made since your last save are lost. StokesForge will close." It also gives the path of a recovery copy, when one could be written. See [Recovery copies](project-files.md#recovery-copies).

## Scripting

### The script failed and my project did not change

**Cause.** This is by design. When a script raises an error, everything that run changed is undone. The status bar says "Script failed. See the console".

**What to do.** Read the **Traceback** in the output area. It names the line that failed. Fix the line and run again. A script that you stop with **Stop** is rolled back in the same way. See [Errors](../scripting/console.md#errors).

### A common mistake: `np` is not defined

**Cause.** `pyst` is loaded for you, but numpy and scipy are not.

**What to do.** Start the script with `import numpy as np`.

### Messages of the script console

| Message | What to do |
|---|---|
| "A script is already running" | Wait for it to end, or click **Stop**. |
| "Wait for the script to finish before saving" | **Save** is not available while a script runs. |
| "The column changed while it was being edited. Try again" | You edited the column by hand between the read and the write of the script. Run it again. |
| "The script engine has stopped. Run the script again" | Run the script again. If it repeats, restart StokesForge. |
| "Interrupt requested" | **Stop** takes effect at the next call to `pyst`. A long numpy calculation finishes first. |

## Copying figures and cells

To paste a graph into another program, use **Graph ▸ Export ▸ Copy Graph**, or <kbd>Ctrl</kbd>+<kbd>C</kbd> when no object of the graph is selected. See [Copy a graph to the clipboard](../guide/export.md#copy-a-graph-to-the-clipboard).

### Nothing is pasted, or the wrong format

**Cause.** StokesForge puts SVG, PNG and PDF on the clipboard together. The program you paste into chooses one. <kbd>Ctrl</kbd>+<kbd>C</kbd> copies the whole graph only when no object is selected and no text field has the focus.

**What to do.** Click an empty part of the graph, or use the menu. To change the formats offered or their order, use **File ▸ Preferences… ▸ Export**. If your program still pastes the wrong format, export a file instead.

### By system

| System | What to know |
|---|---|
| Linux, Wayland | Copy from the StokesForge window with the key or the menu. The desktop accepts a copy only from the window that has the focus, right after you act. |
| Linux, X11 | The copy is available while StokesForge runs and is lost when you close it. A format of about 16 MB or more is left out. |
| Windows | SVG, PNG and PDF are offered as separate clipboard formats. |

### Messages

| Message | What to do |
|---|---|
| "This display cannot receive a copied graph (…)" | Your display system cannot receive the copy. Export a file instead. |
| "There is nothing to copy" | The figure is empty. Check that the graph shows something, then copy again. |
| "Could not copy the graph: …" | Export a file instead, and report the cause if it repeats. |
| "Select a cell to paste into" | Click a cell of the sheet, then paste. |

## Report a problem

To report a mistake in the software or in this manual, open an issue on the
[issue tracker](https://gitlab.com/askrzypczak/stokesforge/-/issues). You need a GitLab account.

Search the existing issues first: your problem may be known. Then write a new issue with:

- **The version** of StokesForge. Open **Help ▸ About StokesForge** and click **Copy** to get a line to paste, or run `stokesforge --version`. If the application does not start, the version is in the name of the package you installed, for example `StokesForge-1.0.0-linux-x86_64.tar.gz`.
- **Your system**: Windows or Linux and its version. On Linux, say whether you use Wayland or X11. For a display problem, name your graphics card and its driver.
- **The steps** to reproduce the problem, one action per line, starting from a new project.
- **What you expected** and **what happened**. Copy the exact text of any message. The status bar keeps the last 50.
- **A file** that shows the problem: a small `.stf` project, or the import file. Remove or replace data that are confidential before you send it.
- **The script**, if the problem comes from a script, and the full **Traceback**.

If the program closed by itself, say so, and say whether the window **Recover Projects** appeared at the next start.
