# Projects and workbooks

This page covers the project file, how to organise it with folders, and how to work with workbooks: sheets, columns, cells, copy and paste, and columns computed by a formula.

To bring instrument files into a workbook, see [Importing data](import.md).

## Projects

A project holds everything you work on: workbooks, graphs, notes, folders and the results of your analyses. It is saved as one file with the extension `.stf`. One StokesForge window holds one project.

### Create a new project

Choose **File ▸ New ▸ New Project** or press <kbd>Ctrl</kbd>+<kbd>N</kbd>. The current project is replaced by an empty one in the same window.

If the current project has unsaved changes, StokesForge asks what to do:

- **Save** saves the project, then goes on. A cell you are still typing is entered first, so it is saved too.
- **Discard Changes** goes on and loses the changes since the last save.
- **Cancel** (or <kbd>Esc</kbd>) keeps the project open.

A project that is still empty is replaced without a question.

### Open a project

Choose **File ▸ Open…** or press <kbd>Ctrl</kbd>+<kbd>O</kbd>, then pick a `.stf` file. The window starts in the default folder set in **File ▸ Preferences… ▸ Folders** the first time, then in the last folder you used. The project replaces the current one, after the same question about unsaved changes.

You can also drop a `.stf` file on the StokesForge window. It opens in a new StokesForge window and leaves your current project as it is.

If the file cannot be opened, a message says so and your current project stays unchanged.

StokesForge refuses to open a very large file, to protect your computer. The limits are set in **File ▸ Preferences… ▸ General** (**Maximum file size (MB)** and **Maximum cells (millions)**). The message names the setting to raise.

### Save a project

- **File ▸ Save** or <kbd>Ctrl</kbd>+<kbd>S</kbd> saves the project in its file. The first time, you choose where to save.
- **File ▸ Save As…** or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> saves under a new name. If the file exists, StokesForge asks before it replaces it.

If you are still typing in a cell, a header field or a note, saving enters what you typed first, in a detached window as well. The same goes for a graph title, legend name or free text you are still editing. A number the cell refuses (out of range) stays open and is not saved.

The title bar shows a dot after the project name while there are unsaved changes. If saving fails, a message explains why and your changes are kept.

**Save** is not available while a script is running.

The project file does not keep the undo history. See [The main window](interface.md#undo-and-redo).

StokesForge has no list of recent projects. Use **File ▸ Open…**.

### Recovery copies

By default, StokesForge writes a recovery copy of your project every 5 minutes while it has unsaved changes. The copy goes to a separate recovery folder. It never overwrites your file.

If StokesForge does not close properly (a crash, a power cut), the next start offers the copies that are newer than the saved file in a **Recover Projects** window. For each copy:

<img src="../images/files-recover-projects.png" alt="The Recover Projects window with two recovery copies, each with Open, Ignore and Delete." width="560">

- **Open** opens the recovery copy. The list stays open for the others. Save the project to keep it.
- **Ignore** keeps the copy. It is offered again at the next start.
- **Delete** removes the copy. It asks for confirmation.

The copy is deleted when you save or quit normally.

To change this, choose **File ▸ Preferences… ▸ General**: **Save a recovery copy automatically** turns it on or off, and **Copy every (min)** sets the period.

## The Project Manager and folders

The **Project Manager** at the left shows the content of the project as a tree. See [The main window](interface.md#project-manager) for how to select and open entries.

Create objects from the **File ▸ New** menu or the **File** toolbar:

| Command | Creates |
|---|---|
| **Add Workbook** | A workbook with one sheet, with an X column and a Y column |
| **Add Graph** | An empty graph with one plot area |
| **Add Folder** | An empty folder at the root of the project |

A new workbook is named `Book1`, `Book2`… A new window opens in the foreground.

A new workbook or graph is created in the folder you have selected in the Project Manager, or in the folder of the selected window. With nothing selected, or several entries, it goes to the top level of the project. That folder opens to show it. In a folder shown sorted it takes its place in the order, in a folder in manual order it goes to the end.

To create a folder inside another folder, right-click the folder in the Project Manager and choose **Add Folder**. The new folder opens with its name ready to edit. A right-click in the empty area below the last row holds **Add Folder**, **Paste** and **Sort** for the top level. A click in that empty area deselects everything, so the next new workbook or graph, or **Paste**, goes to the top level.

Folders contain workbooks, graphs and other folders. Names must be unique inside a folder.

### Rename, duplicate and delete

Select one or more entries in the Project Manager, then use the right-click menu or **Edit ▸ Selection**:

| Command | Shortcut | What it does |
|---|---|---|
| **Rename** | <kbd>F2</kbd> | Edits the name in place. <kbd>Enter</kbd> confirms, <kbd>Esc</kbd> cancels. With several entries selected, you name them one after the other |
| **Duplicate** | <kbd>Ctrl</kbd>+<kbd>D</kbd> | Makes a copy named `… copy 1`, `… copy 2`… (duplicating a copy counts on: `Book copy 1` gives `Book copy 2`). A copied graph keeps using the original data, unless its workbook is copied with it (a folder) |
| **Delete** | <kbd>Del</kbd> | Removes the entries and closes their windows |
| **Cut** / **Paste** | <kbd>Ctrl</kbd>+<kbd>X</kbd> / <kbd>Ctrl</kbd>+<kbd>V</kbd> | Moves the entries into the selected folder, in one step that Undo reverts. The cut rows, text and icon, are dimmed until you paste. A cut pastes only once: pasting again does nothing until you copy or cut something. Pasted into another project, a cut copies instead of moving. A folder cannot go into itself, and a name already used in the destination refuses the whole move |
| **Copy** / **Paste** | <kbd>Ctrl</kbd>+<kbd>C</kbd> / <kbd>Ctrl</kbd>+<kbd>V</kbd> | Copies the entries, also to another project open in another window. A pasted entry keeps its name unless the folder already holds it, then it becomes `… copy 1` |

An empty name is refused and the previous name is kept.

### Move entries by dragging

Drag an entry, or several selected entries, onto a folder to move them into it. Drag them between two rows to put them there. Release below the last row to move them to the top level. The folder under the pointer is highlighted, or a line shows the place. A closed folder opens after a second. Press <kbd>Esc</kbd> before releasing to cancel. One **Undo** puts everything back. The same rules as Cut and Paste apply: a folder cannot go into itself, and a name already used in the destination refuses the whole move.

### Order of a folder

Every folder shows its entries sorted: folders first, then by name, with numbers in numeric order (`Fit2` before `Fit10`). New entries and renamed entries take their place in that order.

When you drag entries between two rows, that folder switches to **manual order**: it keeps the order you made, and new entries arrive at the end. Right-click the folder and choose **Sort** to sort it again. For the top level, right-click the empty area below the last row. **Sort** is greyed out when the folder is already sorted. The manual orders are saved with the project. Copying or duplicating a folder in manual order gives a copy that keeps its manual order, and so do the manual folders inside it.

A folder can be copied and duplicated with everything it holds. What is inside the copy reads the copy: a graph in the copied folder plots the copied workbook, not the original. What is outside the folder is still read from the original. The same holds for several windows copied together: if you select Book1 and Graph1 and copy them in one gesture, the copied Graph1 plots the copied Book1. **Paste** goes where a new workbook would: select only a folder to paste into it, select a window to paste into its folder, select nothing to paste at the top level. To paste into a folder, select only that folder and choose **Paste**: the folder opens to show what arrived. If the folder you copied is itself selected, **Paste** puts the copy next to it.

**Delete** asks no confirmation. If graphs used the deleted data, the status bar says so. Undo brings everything back. Undo does not reopen the windows.

## Sheets

A workbook holds one or more sheets. Click a sheet tab to show that sheet.

When there are more tabs than fit, **<** and **>** buttons appear at the left of the strip to show the hidden tabs.

| To… | Do this |
|---|---|
| Add a sheet | Click **Add Sheet** in the sheet tabs, or right-click the empty strip, or use **Workbook ▸ Sheet** |
| Rename a sheet | Double-click its tab, or right-click it and choose **Rename**. <kbd>Enter</kbd> confirms, <kbd>Esc</kbd> cancels |
| Duplicate a sheet | Right-click its tab and choose **Duplicate**, or press <kbd>Ctrl</kbd>+<kbd>D</kbd>. The copy is named `… copy 1` and is shown |
| Delete a sheet | Right-click its tab and choose **Delete** |

The same commands are in **Workbook ▸ Sheet**.

<img src="../images/workbooks-workbook-menu.png" alt="The Workbook menu of the menu bar: Sheet and Columns (submenus), Plot (greyed out), and the check boxes Show Metadata Row (checked) and Show x' Row." width="156">

A sheet name cannot be empty and must be unique inside the workbook. A workbook always keeps at least one sheet: **Delete** is greyed out on the last one.

Deleting a sheet can be undone. If a graph plotted columns of that sheet, the status bar names it.

**Reset Column Widths** (in the same menu) puts every column of the sheet back to its default width.

## Columns

Each column has a letter (A, B, … Z, AA, AB…), a type and a role. The header above the data has four rows to describe the column.

<img src="../images/workbooks-sheet.png" alt="The Spectrum sheet of Book1: the header rows (Long Name, Units, Comments, F(x)=, Metadata), the column letters with their roles, and the sheet tabs at the bottom." width="558">

### The column header

| Row | Content |
|---|---|
| Letter | The position of the column, followed by its role: `A(X)`, `B(Y)` |
| **Long Name** | A description of the column, for example `Wavelength` |
| **Units** | The unit of the values, for example `nm` |
| **Comments** | Free text |
| **F(x)=** | A formula that computes the column (see [Columns computed by a formula](#columns-computed-by-a-formula)) |

Double-click a header field, or select it and press <kbd>F2</kbd>, to edit it. <kbd>Enter</kbd> confirms.

Click a label at the left of the header (for example **Long Name**) to select that field for every column of the sheet.

#### Header rows

Two more rows can be shown above these four:

- **Metadata**: a summary of the information read from the imported file. Click it to list the keys (**File Metadata**).
- **x'**: a number attached to each column, for example the time of a measurement. It is used to plot rows.

Choose **Workbook ▸ Show Metadata Row** or **Workbook ▸ Show x' Row** to show or hide them in the active workbook. These choices apply to this workbook until you close StokesForge. The setting for new workbooks is in **File ▸ Preferences… ▸ Display ▸ New**.

### Add a column

Choose **Workbook ▸ Columns ▸ Add Column**, click **Add Column** in the **Tools** toolbar, or right-click the empty space at the right of the last column and choose **Add Column**. The new column goes at the end of the sheet. You can move it afterwards.

Pressing <kbd>Tab</kbd> in the last column of a row adds a new column and moves to it.

### Select columns

- Click the column letter to select the column.
- <kbd>Ctrl</kbd>+click adds or removes a column. <kbd>Shift</kbd>+click selects a range.
- Click the **#** cell at the top left, or press <kbd>Ctrl</kbd>+<kbd>A</kbd>, to select the whole sheet. Click it again to clear the selection.

<img src="../images/workbooks-column-menu.png" alt="The right-click menu of a column letter (here B(Y)): Plot, Add Column, Analysis, clipboard commands, Move, Type, Role, Reset Width, Clear and Delete." width="558">

*Right-click a column letter to open this menu. The other column commands below are in it.*

### Move columns

Select one or more columns, then choose **Move** in **Workbook ▸ Columns** or in the right-click menu of the column letter:

| Command | Effect |
|---|---|
| **Move to First** | Moves the selected columns to the start of the sheet, keeping their order |
| **Move Left** | Moves each selected column one place left |
| **Move Right** | Moves each selected column one place right |
| **Move to Last** | Moves the selected columns to the end of the sheet |

A command that would change nothing is greyed out. Columns move inside their sheet only.

Graphs that are already plotted do not change when you move a column. A formula keeps reading the same columns, and its letters in the **F(x)=** row follow the moved columns.

### Delete columns

Select the columns and right-click a letter, then choose **Delete**. Undo brings the columns back with their values.

### Column type

A column has a type. The grid shows it by alignment: numbers on the right, text on the left.

| Type | Holds |
|---|---|
| **Numeric** | Numbers. A cell may be empty |
| **Mixed** | Numbers, text and empty cells, each cell by itself |
| **Date/Time** | Dates and times, kept as numbers |

When you type text that is not a number into a **Numeric** column, the column becomes **Mixed**: the text is kept and the other numbers stay numbers. Undo restores the type and the value together. If you then overwrite all the text, the column returns to **Numeric**.

To change a type by hand, select the columns and choose **Type** in the right-click menu of the letter (or **Workbook ▸ Columns ▸ Type**), then **Numeric** or **Mixed**. The entries are greyed on a column computed by a formula or produced by a calculation: remove the formula, or detach the column from the calculation, to change its type. In a selection that also holds other columns, only those change.

Converting to **Numeric** erases the cells that are not numbers. Undo brings them back.

### Column role

The role says what the column is used for when you plot. Select the columns, then choose **Role** in the right-click menu of the letter or in **Workbook ▸ Columns ▸ Role**:

- **X**, **Y**: the main X and Y.
- **X2**, **Y2**, **X3**, **Y3**, **X4**, **Y4**: the columns of the extra layers of a graph.
- **Z**.
- **X Error**, **Y Error**: error bars.
- **Label**.
- **None**.

The role is shown after the letter, for example `A(X)`. See [Graphs](graphs.md) for how the roles are used when you plot.

### Column width

Drag the right edge of a column letter to change the width. The width stays with the column when you add, delete or move other columns, and it is saved in the project.

To restore the default, right-click the letter and choose **Reset Width**. Changing a width cannot be undone.

## Cells

### Edit a cell

1. Click a cell to select it.
2. Type the new value, or press <kbd>F2</kbd> or double-click to edit the current value.
3. Press <kbd>Enter</kbd> to confirm, or <kbd>Esc</kbd> to cancel.

Use the arrow keys to move between cells. <kbd>Tab</kbd> moves to the next column. <kbd>Enter</kbd> on the last row moves to the entry row.

### Add a row

Below the last row, the grid always shows an empty **entry row**, marked `*` in the row-number column (hover it: *Type here to add a row*). It is there in a new workbook too, which has no rows yet. Click a cell of the entry row and type a value, then press <kbd>Enter</kbd>: the row is created, and the cursor moves to the next entry row. One **Undo** removes the row. The entry row is not data: it is not saved, not plotted, and **Select All** does not include it. A range extended with <kbd>Shift</kbd> (click, drag or arrows) stops at the last row and never includes the entry row.

A decimal comma is accepted: `1,5` is read as 1.5. The status bar tells you when it did so. A number typed in a cell beyond about 1.8e308, or non-zero but too small to be stored (under about 4.9e-324), is refused with the same message as in a formula and the cell stays open; a pasted block that holds such a number is refused as a whole (nothing is pasted, and the status bar names the number and its row); imported values are not checked. An empty cell is a missing value. Dates and times are read when the column is **Date/Time**.

### Select cells

- Drag to select a block.
- <kbd>Shift</kbd>+click extends the last block.
- <kbd>Ctrl</kbd>+click adds or removes a cell, and <kbd>Ctrl</kbd>+drag adds a block.
- Click a row number to select the whole row. Click a column letter to select the whole column.

### Clear and delete

| Key | Effect |
|---|---|
| <kbd>Backspace</kbd> | Clears the selected cells |
| <kbd>Del</kbd> | Clears the selected cells. Press it a second time on the same selection to delete them and move the cells below up |

The right-click menu has **Clear** and **Delete and Shift Up**. **Delete and Shift Up** acts at once. It is offered for a single block of cells.

### Copy and paste

| Command | Shortcut | What it does |
|---|---|---|
| **Cut** | <kbd>Ctrl</kbd>+<kbd>X</kbd> | Copies, then clears |
| **Copy** | <kbd>Ctrl</kbd>+<kbd>C</kbd> | Copies the selected cells |
| **Copy with Header** | | Copies the values and the column header |
| **Paste** | <kbd>Ctrl</kbd>+<kbd>V</kbd> | Pastes at the selected cell |
| **Paste in Place** | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd> | Pastes at the position the cells were copied from |

<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> copies the whole columns that the selection touches.

Copied cells are placed on the system clipboard as tab-separated text. You can paste them into a spreadsheet or a text editor, and paste from a spreadsheet into StokesForge.

A few rules:

- **Paste** needs a single block of cells or a run of contiguous columns as its target.
- **Copy** and **Cut** work on several blocks only if they share their rows (side by side) or their columns (stacked). Otherwise they are greyed out.
- Pasting over a **Mixed** column that ends up with only numbers turns it back to **Numeric**.
- If part of the pasted data does not fit, the status bar says how many columns or header fields were left out.

A paste is one action for Undo.

On macOS, use <kbd>Cmd</kbd> in place of <kbd>Ctrl</kbd>.

## Columns computed by a formula

A column can compute its values from other columns of the same sheet. Type the formula in the **F(x)=** row of the column.

For example, to fill column C with the sum of A and B, double-click the **F(x)=** cell of column C, type `A+B` and press <kbd>Enter</kbd>.

While you type, a small **?** icon appears in the next cell, just after the line that ends the one you are editing. Click it to open the **Formula Functions** window: the operators and functions you can use, one family at a time (pick the family in the tabs at the left), each with its signature, an example and what it does. Press <kbd>Esc</kbd> or **Close** to leave it.

<img src="../images/workbooks-formula-help.png" alt="The Formula Functions window on the Column values family: the families as tabs at the left, a table of Function, Example and What it does at the right." width="900">

If you misspell a function name, the message proposes the nearest one (`sqr(A)` gives "Did you mean sqrt?"), and a wrong number of values names the function to write instead (`log(B, 2)` gives "Use logx instead"). You can hide the icon in **Preferences ▸ Display ▸ Interface** (**Show F(x)= help**).

### Formula syntax

- Columns are written with their letters: `A`, `B`, `AA`. Capitals or lowercase both work.
- Numbers: `2`, `0.5`, `1e-3`.
  A number beyond about 1.8e308, or non-zero but too small to be stored (under about 4.9e-324), makes the formula uncalculated: it is kept and framed, with a message that gives the limit. Write `0` for zero.
- Operators: `+`, `-`, `*`, `/`, `^` (power), and a minus sign in front of a value (`-A`).
- Parentheses: `(A+B)/2`. `^` is done first, then `*` and `/`, then `+` and `-`. `^` reads from the right (`2^3^2` is 512) and binds tighter than a minus sign (`-2^2` is -4).
- Comparisons: `<`, `<=`, `>`, `>=`, `==`, `!=`. They give 1 when true and 0 when false, and are done last.
- Most functions work on one row at a time. Write the name, then the values in parentheses, separated by commas: `sqrt(A)`, `round(B, 2)`. A few give one value for a whole column: see [Values of a whole column](#values-of-a-whole-column).

Do not start the formula with `=`.

A name followed by `(` is a function, in capitals or lowercase (`EXP(B)` is `exp(B)`). A name on its own is a column (`b/2` is column B divided by 2). The number π is written `pi()`. There is no implicit multiplication: write `A*(B/C)`, not `A(B/C)`.

| Family | Functions |
|---|---|
| Powers, logarithms | `sqrt(x)`, `root(x, n)` (`root(-8, 3)` is -2), `exp(x)`, `ln(x)`, `log(x)` (base 10, same as `log10(x)`), `log2(x)`, `logx(x, b)` (base `b`), `pow(x, y)` |
| Absolute value, sign | `abs(x)`, `sign(x)` |
| Trigonometry | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2(y, x)`, `sinh`, `cosh`, `tanh`, `pi()` |
| Rounding | `round(x, n)` (half away from zero on the number as displayed: `round(1.005, 2)` is 1.01, like Excel; `n` may be negative: `round(1234, -2)` is 1200), `floor(x)`, `ceil(x)`, `mod(a, b)` (sign of `b`: `mod(-7, 3)` is 2; a tiny negative `a` can give `b` itself, like Python and Excel: `mod(-1e-20, 3)` is 3) |
| Two values | `min(a, b)`, `max(a, b)` (row by row; with one column or three values, see below) |
| Whole column | `at(B, x)`, `max(B)`, `min(B)`, `mean(B)`, `sum(B)`, `std(B)`, `integral(B)`, and their range forms: see below |
| Conditions | `if(condition, a, b)`: `a` when the condition is true (not 0), otherwise `b` |
| Others | `erf(x)`, `erfc(x)` |

Examples: `-log(B)` (absorbance from a transmittance), `sqrt(A)`, `if(B<0, 0, B)`, `A*pi()`.

In `if`, only the chosen value is read: an empty cell in the other one does not matter.

### Values of a whole column

Some functions read a whole column and give **one value**, the same on every row, which you can use in a formula that works row by row. The classic use is normalizing a spectrum:

| Formula | Result |
|---|---|
| `B / at(B, 340)` | B divided by its value at X = 340 (1 on the 340 row) |
| `B / max(B, 330, 350)` | B divided by its peak between 330 and 350 |
| `B / integral(B)` | B divided by its area |
| `B - mean(B)` | B centered on its mean |
| `(B - mean(B)) / std(B)` | B centered and scaled |

- `at(B, x)` is the value of B at X = `x`, by straight-line interpolation between the two neighboring points. Outside the X values the cell is empty (no extrapolation). `x` can be a column: `at(B, A)` reads `x` row by row. `at(B, x, D)` reads the X values in column D.
- `max(B)`, `min(B)`, `mean(B)`, `sum(B)` and `std(B)` are computed over the whole column. `std` is the sample standard deviation (divided by n − 1) and needs two values.
- `integral(B)` is the area under B by the trapezoid method, the same result as [Integration](analysis.md#integration) without a baseline.
- **Range**: add two X values, in any order, to keep only the points between them: `max(B, 330, 350)`, `mean(B, 400, 500)`, `integral(B, 300, 400)`. The bounds can be columns: `max(B, D, E)` takes, on each row, the points whose X is between D and E of that row. An infinite bound (`1/0`, `-1/0`) means "to the end": `max(B, 330, 1/0)` is the peak from 330 on. Three values are always a range (`max(B, D, E)` is not the maximum of three columns); `max(B, C)` stays the row-by-row maximum of two columns, and `max(max(B, D), E)` the maximum of three. **Decimal comma**: `max(B, 1,5)` is the range from 1 to 5, not the number 1.5. When two plain numbers are separated by a comma without a space in such a call (`max(B,330,350)`), StokesForge keeps the range, rewrites the call with spaces (`max(B, 330, 350)`) and shows the **F(x)=** cell in orange for 10 seconds with the reason, which is also in the status bar. For the number 1.5, write it with a point.
- The first value of these functions is a **column letter**, not a calculation: `max(B*2)` is refused. Compute the new column first, then read it.

The X is the column of role X nearest to the **left** of B, the same rule as the graph. `at`, `integral` and the range forms need it; `max(B)` and the other whole-column statistics do not. If B has no X, the formula is kept, its cells show `---` and its **F(x)=** cell is framed in red with the reason; it computes as soon as an X appears (change a column role, or move the X to the left of B). Rows whose X is empty or text are ignored. Empty cells, text and infinite values in B are ignored too.

When there is no point to compute from (an empty column, an empty range, a value outside the X values), the cell is empty, for `sum` and `integral` as well (never 0). The one exception is `integral` over a zero-width range (`integral(B, 2, 2)`) when that X value lies between the smallest and the largest X of the column: it is 0; outside them (`integral(B, 100, 100)` with X from 0 to 2) the cell is empty. If the whole result column is empty for that reason, the **F(x)=** cell is framed in red.

The value is computed once for the whole column, not once per row, so `B / max(B)` stays fast on long columns. It is recalculated when B, its X, or the bounds change, and also when you change the role of a column or move one so that B has another X.

#### Columns of another sheet or workbook

A whole-column function can read a column of **another sheet**, or of **another workbook**. Write the sheet before the column letter, joined with `!`:

- `B` is a column of this sheet;
- `Sheet3!B` is a column of another sheet of the same workbook;
- `[Book2]Sheet5!B` is a column of a sheet of another workbook. A workbook always comes with its sheet: `[Book2]B` is refused, write `[Book2]Sheet1!B`.

The reference is one value of the function, so the range stays on the right: `at(Sheet3!B, 340)`, `integral(Sheet3!B, 10, 100)`, `max([Book2]Sheet5!B, 330, 350)`, and `B / max(Sheet3!B)` to normalize a column by the maximum of another sheet. The functions that accept it are `at`, `max`, `min`, `mean`, `sum`, `std` and `integral`. `Sheet3!B + 1`, `sqrt(Sheet3!B)` and `max(Sheet3!B, C)` (the row-by-row maximum) are refused: such a reference is read by a whole-column function only.

- **Quotes.** A sheet name that holds anything other than ASCII letters and digits (a space, `_`, an accent, `ᵀ`), or that starts with a digit, is written between single quotes: `'Blanc 2'!B`. A quote inside the name is doubled: `'it''s'!B`. A workbook name between brackets needs no quotes.
- **Case.** Sheet and workbook names are read without regard to case: `sheet3!b` is `Sheet3!B`, and the field shows the real name. For the same reason, two objects next to each other can no longer differ only by case: `SHEET1` is refused beside `sheet1` (sheets, workbooks, folders, graphs, notes). Column names are not concerned. A project saved earlier that holds such names still opens; a formula that names one of them is refused as ambiguous.
- **Which workbook.** `[Book2]` is looked up in the folder of the workbook that holds the formula first, then in the whole project. If several workbooks outside that folder carry the name, StokesForge asks for the path from the project root: `[Mesures/Book2]Sheet5!B` (`[/Book2]Sheet5!B` for a workbook at the root). The field always shows the shortest form that is not ambiguous.
- **The X column.** The X of a column of another sheet is the one paired with it **in its own sheet**. In `at(Sheet3!B, x, D)`, `D` is a column of `Sheet3` and `x` is calculated in this sheet, row by row; an X column written with its own prefix (`at(B, x, Sheet3!D)`) is refused.
- **The result.** The result column has the length of its own sheet, whatever the length of the column read. A sheet that holds only formulas of this kind (or constants such as `1+2`) has one row, so the result shows; it takes the length of the sheet as soon as the sheet has data columns.
- **Renaming and moving.** The formula follows the sheet: renaming `Sheet3`, or moving it to another workbook (the field then shows `[Book2]Sheet3!B`), changes the text of the field and keeps the values. If the sheet or the column is deleted, or the column is moved to another sheet, the formula shows `#REF` in red and its cells are empty; undoing brings the values back.
- **Loops.** A formula that would read itself, directly or through other formulas of any sheet, is kept but not calculated: its cells are empty and its **F(x)=** cell is framed in red with "Formulas refer to each other in a loop. Remove one of the references". Every column of the loop is framed, and a formula that reads one of them is empty too.
- **Copies.** A copied sheet keeps reading the other sheets it did not copy. When you copy a workbook, a formula reads the copy of the sheet that was copied with it. Pasted into another project, the reference becomes `#REF`.

If you need functions of the data, you can use a script (see [The pyst API](../scripting/api.md)), or use the [Analysis](analysis.md) tools.

### How a formula works

- The formula is calculated row by row (the whole-column functions above are computed once, then used on every row). An empty cell in a source gives an empty cell in the result.
- A result that has no value (`ln(-1)`, `sqrt(-1)`, `mod(a, 0)`, `root` of a negative number with an even `n`, `0/0`, infinity minus infinity, zero times infinity) is an empty cell, shown as a failed calculation. Dividing a non-zero number by zero is not an error: `1/0` is infinity.
- The values of the column are read-only. To change them, edit the formula.
- The formula keeps its columns even if you add, delete or move others. The letters in the **F(x)=** row follow the columns.
- If a column the formula reads is deleted, the formula shows `#REF` in red. Replace `#REF` with a column letter.
- The result is recalculated when its source data change.

### Messages

A formula you type is **always kept**: Enter, Tab or a click elsewhere closes the field and the text stays as you wrote it. When StokesForge cannot calculate it, the column is empty, the **F(x)=** cell is framed in red, the reason shows in a tooltip and in the status bar, and **Ctrl+Z** brings back the previous formula and its values. The reason is one of:

- a letter names no column in the sheet;
- a sheet or a workbook of a reference does not exist, or the sheet has no such column;
- a reference to another workbook does not name its sheet, or its workbook name is carried by several workbooks (write the path);
- a column of another sheet is used outside a whole-column function, or as the X column of `at`;
- the formula would read itself through other formulas;
- the formula uses the column that holds it;
- the text still contains `#REF`;
- a name followed by `(` is not a function;
- a function gets the wrong number of values;
- a column letter is followed by `(` (`A(B/C)`): write `A*(B/C)`;
- the column of a whole-column function is an expression: `max(B*2)`, `at(B, 1, A+1)`;
- `pi` is written alone and the sheet has no column PI: write `pi()`;
- a number is too large or too small to be stored;
- the formula is incomplete or not valid (`A +`, a missing parenthesis): check the operators and the parentheses.

While you type, the cell turns red as soon as the text is known to be wrong, except for an incomplete formula: `A +` stays plain until you close the field. The kept text keeps the columns it could read: in `A+Z` with no column Z, the column A still follows moves.

A formula kept because a column or a sheet it names does not exist yet **computes by itself when that column or sheet appears**: add the column F that `A+F` waits for, rename a sheet to the name a formula uses, move a column into the sheet, delete the duplicate workbook that made a name ambiguous, or open a file where it is now there. The status bar says, in orange and for 30 seconds (or until another action says something), which column it now reads (for example "The formula of column E now reads A+F", or how many formulas were bound). The letter you typed means the column that holds that position when the column appears, so check the message. The binding belongs to the action that created the column: **Ctrl+Z** takes both back, and the formula is kept again. Once bound, the formula holds the column like any formula, so moving it later breaks nothing. A formula that is still wrong elsewhere (`#REF+F`) keeps waiting for everything, and a binding that would make a loop stays framed until the loop is broken.

A script (`pyst`) is stricter than the field: a script that writes a formula the field would keep gets an `InvalidValueError`, and nothing is written. A paste behaves exactly like typing, including the rewrite of a range written with a decimal comma (the text becomes `max(B, 1, 5)`, the cell turns orange for 10 s and the status bar says so; **Ctrl+Z** clears it). A pasted formula that cannot be read in the destination is kept as it is, framed in red with the reason, and its column is emptied (the copied values are never pasted in its place); the other columns of the paste go through, and the status bar names the columns that do not calculate, then gives the reason for the first one ("The formulas of columns B, C do not compute. B: …"). It calculates by itself if the missing column or sheet appears later, and one **Ctrl+Z** gives back everything the paste replaced.

### Keep the values, remove the formula

To turn a computed column into ordinary data, right-click its letter and choose **Remove Formula, Keep Data**. It is also in **Workbook ▸ Columns**. Emptying the **F(x)=** field removes the formula and the values with it.

When you use **Copy with Header** on a formula column, StokesForge asks whether to copy its **Values** or its **Formulas**.

Pasting a Formulas copy writes the formula; the column's values are recomputed. It does the same when you paste on any data cell of the column, not only on its **F(x)=** field.

A sheet name in a pasted formula is read again in the destination: `Sheet3!B` pasted into another workbook reads that workbook's `Sheet3`.

## Linked sheets

Some sheets are created for you from another sheet.

When you analyse the rows of a sheet (see [Analysis](analysis.md)), StokesForge creates a transposed sheet named after its source, for example `Sheet1ᵀ`. It holds the x' values and one column per row analysed.

A band at the top of this sheet tells you where it comes from, and holds the box **Recalculate automatically**, checked by default: the sheet then follows its source by itself. Uncheck it to keep a snapshot of the source:

- **Out of date** appears when the source has changed since (or, with the box checked, when a column of the source cannot be followed, because it has no x' value, has text, or has a formula that reads this sheet: the band names it in full on a second line, in orange, under the first one; the line goes away once the column is fixed).
- **Values edited** appears when you changed a value of the linked columns.
- **Recalculate** rewrites the linked columns from the source. If you have edited values, the button is **Relink** and asks for confirmation, because it erases your edits. Undo brings them back.

Only the columns with the role **Y** are transposed. A Y column needs an x' value and no text. If a column does not meet this, StokesForge names the column by its letter.

If you delete the source or the transposed sheet, the other one becomes an ordinary sheet.

Columns produced by an analysis are also linked to their calculation. See [Analysis](analysis.md).
