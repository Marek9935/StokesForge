# Your first project

This page walks you through a first session with StokesForge. You import a small spectrum, look at it in a workbook, plot it, give the axes titles with units, measure a peak area, export the figure and save the project.

It takes about fifteen minutes. Each step names the menus and buttons you use, and links to the page of the manual that explains them in depth.

You need StokesForge installed. See [Installation](installation.md).

## The data file

You need a text file to import. Copy the lines below into a text editor and save them as `spectrum.csv`. It holds the absorbance of a sample between 400 nm and 500 nm, with one peak at 450 nm.

```text
Wavelength,Absorbance
400,0.020
405,0.022
410,0.026
415,0.038
420,0.069
425,0.134
430,0.247
435,0.410
440,0.593
445,0.742
450,0.800
455,0.742
460,0.593
465,0.410
470,0.247
475,0.134
480,0.069
485,0.038
490,0.026
495,0.022
500,0.020
```

The first line holds the names of the two columns. The values are separated by commas and use a dot as the decimal separator.

## Create the project

### 1. Start a new project

Start StokesForge, then choose **File ▸ New ▸ New Project** (<kbd>Ctrl</kbd>+<kbd>N</kbd>).

The title bar of the window shows **Untitled project**. The first time you start StokesForge it opens a demonstration project; this step replaces it with an empty project. If StokesForge asks about unsaved changes, choose **Discard Changes**.

See [Projects](../guide/projects-and-workbooks.md#projects).

## Import the data

### 2. Open the Import Wizard

Choose **File ▸ Import ▸ Import…**. The **Import Wizard** opens on its first step, **Files and Template**.

Click **Add Files…** and pick `spectrum.csv`. The preview on the right shows the lines of the file.

Click **Next** to go to the second step, **Format**. The default settings suit this file: the **Delimiter** is **Auto** and the **Decimal separator** is **Dot**. Click **Next** again.

See [The Import Wizard at a glance](../guide/import.md#the-wizard-at-a-glance).

### 3. Tell the wizard about the names

You are now in the third step, **Header and Data End**.

Tick **Names in first line**. The first line of the file gives the column names, so it must not be read as data. Check in the preview that the first line is no longer shown as a data line.

See [Names in first line](../guide/import.md#names-in-first-line) and [The live preview](../guide/import.md#the-live-preview).

### 4. Choose the destination and finish

Click **Next** until you reach the last step, **Destination**. Leave the other steps as they are.

Under **Destination**, choose **New workbook per file**. Click **Finish**.

A workbook opens with your data. The status bar tells you that one file was imported.

See [Where the data goes](../guide/import.md#where-the-data-goes).

## Look at the workbook

### 5. Read the workbook

The workbook shows two columns:

- Column `A(X)` holds the wavelengths. It has the role **X**.
- Column `B(Y)` holds the absorbances. It has the role **Y**.

The **Long Name** row above the data holds the names from the first line of the file: Wavelength and Absorbance.

If you made a mistake in the wizard, choose **Edit ▸ Undo**. The whole import is undone in one step, and you can start again from step 2.

See [Columns](../guide/projects-and-workbooks.md#columns).

### 6. Add the units

The **Units** row is empty. Fill it in:

1. Double-click the **Units** cell of column A, type `nm` and press <kbd>Enter</kbd>.
2. Double-click the **Units** cell of column B, type `a.u.` and press <kbd>Enter</kbd>.

See [The column header](../guide/projects-and-workbooks.md#the-column-header).

## Make a graph

### 7. Plot the spectrum

Click the letter **B** above the second column to select it. Right-click the letter and choose **Plot**, then **Line**.

A graph opens in a new window. It draws column B against column A, because the X values of a Y column come from the nearest X column to its left.

See [Plot columns from a workbook](../guide/graphs.md#plot-columns-from-a-workbook).

### 8. Write the title of the X axis

Double-click the X axis, the horizontal line at the bottom of the graph. The **Graph Properties** window opens on the **Axes** tab, on the axis **X (bottom)**.

In the section **Axis title**:

1. Type `$lambda$` in **Quantity**.
2. Type `nm` in **Units**.

The title of the axis becomes λ (nm). Text between dollar signs is math, and `lambda` is the Greek letter. The change shows on the graph at once.

Do not click **OK** yet.

See [Axis title](../guide/graphs.md#axis-title) and [Titles and formulas](../guide/graphs.md#titles-and-formulas).

### 9. Write the title of the Y axis, with a subscript

In the same window, pick the axis **Y (left)** at the top of the **Axes** tab.

1. Type `Absorbance of $"Ru(bpy)"_3$` in **Quantity**. The `_3` makes a subscript. The quotes keep `Ru(bpy)` in upright letters.
2. Type `a.u.` in **Units**.

The title becomes Absorbance of Ru(bpy)₃ (a.u.). If a field shows a red frame after you leave it, the formula has a mistake. Check the dollar signs and the quotes.

Click **OK**. All your changes count as one step for **Edit ▸ Undo**.

See [Rules to remember](../guide/graphs.md#rules-to-remember).

## Measure a peak

### 10. Integrate the peak

Bring the workbook to the front: click its name in the **Project Manager** at the left. Click the letter **B** to select the Y column, then choose **Analysis ▸ Integrate…**.

The window **New analysis: Integrate** opens. It shows the curve, with the settings on the left.

1. Click **Add Range**. Type `420` in the first bound and `480` in the second. The two bounds are separated by **to**.
2. Choose **Area above the line between the bounds**. This measures the peak above the straight line that joins the curve at 420 nm and 480 nm.
3. In the **Output** block, choose **Keep in Results**. If **Add to graph** is ticked, untick it, so that the graph keeps only the spectrum.

Under the preview, **Area 1** shows the area of the peak. It is close to 21.5. Click **Create**.

A **Results** pane appears under the sheet, collapsed to a banner, **Results (1)**. Click its arrow to read the value again.

The analysis is saved with the project. You can reopen it later with **Edit Calculation…**. See [Integration](../guide/analysis.md#integration) and [The Results pane](../guide/analysis.md#the-results-pane).

## Export and save

### 11. Export the figure

Bring the graph to the front: click its name in the **Project Manager**. Choose **Graph ▸ Export ▸ Export Graph…** (<kbd>Ctrl</kbd>+<kbd>E</kbd>).

In the **Export Graph** dialog, pick a folder and type the name `spectrum.svg`. The extension decides the format: `.svg`, `.pdf` or `.png`. Confirm.

The status bar shows **Graph exported to** followed by the path. Open the file in a vector editor or a browser to check it.

See [Export a graph to a file](../guide/export.md#export-a-graph-to-a-file).

### 12. Save the project

Choose **File ▸ Save** (<kbd>Ctrl</kbd>+<kbd>S</kbd>). The first time, StokesForge asks where to save. Type a name such as `first-project` and confirm.

The project is saved as `first-project.stf`. The dot after the name in the title bar disappears.

The file holds your data, the graph, the integration and the position of your windows. It does not hold the undo history. See [Project files](../reference/project-files.md).

To come back later, choose **File ▸ Open…** and pick the file.

## Where to go next

- [The main window](../guide/interface.md): the zones of the window, undo and preferences.
- [Importing data](../guide/import.md): save a template for the files of an instrument.
- [Graphs](../guide/graphs.md): overlay layers, series settings, texts and shapes.
- [Analysis](../guide/analysis.md): baselines, smoothing, fits and peak decomposition.
- [The script console](../scripting/console.md): do the same work with a Python script.
- [Troubleshooting](../reference/troubleshooting.md): when something does not work as described.
