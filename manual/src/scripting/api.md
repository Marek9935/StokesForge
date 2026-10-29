# The pyst API

`pyst` is the module that scripts use to read and change a StokesForge project. This page lists its functions by task, with the arguments, the return values and short examples. To run a script, see [The script console](console.md).

Scripts written against the functions on this page keep working in later versions of StokesForge. Functions can be added, but the ones listed here keep their names, arguments and results.

## Before you start

### Handles

Most functions take or return a **handle**. A handle is an opaque token that points to one object of the project: a folder, a workbook, a sheet, a column, a graph or a note. You cannot create or read a handle yourself. You get one from `pyst.resolve`, `pyst.children` or a function that creates an object.

A handle follows its object. It stays valid when you rename or move the object, and after you delete it and then undo the deletion. It becomes invalid when another project is opened. Using an invalid handle raises `BrokenReferenceError`.

### Paths

`pyst.resolve` finds an object from its path. A path has this form:

| Path | Object |
|---|---|
| `[Book1]` | The workbook **Book1** |
| `[Book1]Sheet1` | The sheet **Sheet1** of **Book1** |
| `[Book1]Sheet1!B` | The column named **B** in that sheet |
| `/Runs/[Book1]Sheet1!B` | The same, when **Book1** is in the folder **Runs** |
| `/Runs` | The folder **Runs** |

Names are case-sensitive. A path without a folder works if the workbook name is unique in the project. The column name in a path is its name in the header (A, B, C… or the name you gave it), not its long name.

### Errors

Four kinds of errors can be raised. All of them derive from Python's `Exception`.

| Error | Raised when |
|---|---|
| `pyst.UnknownPathError` | A path is malformed or points to nothing |
| `pyst.BrokenReferenceError` | A handle points to an object that no longer exists, or comes from another project |
| `pyst.InvalidValueError` | An argument is refused: a wrong type, an unknown option, a name already taken, a file that cannot be read… |
| `pyst.ConcurrentWriteError` | You write a column that changed since your script read it |

A script can catch them:

```python
try:
    sheet = pyst.resolve("[Book1]Sheet1")
except pyst.UnknownPathError:
    print("Book1 has no Sheet1")
```

### Undo

All the changes of one run are one undo step. See [How scripts change the project](console.md#how-scripts-change-the-project).

## Navigate the project

### Find an object

`pyst.resolve(path)` returns the handle of the object at `path`.

```python
column = pyst.resolve("[Book1]Sheet1!A")
```

`pyst.root()` returns the handle of the root folder of the project.

### Walk through the project

`pyst.children(handle=None)` returns the list of the direct children of an object, in project order. Without an argument, it returns the children of the root.

- A folder has folders, workbooks, graphs and notes as children.
- A workbook has its sheets as children.
- A sheet has its columns as children.
- A column, a graph and a note have no children: the list is empty.

The list does not update by itself. Call `pyst.children` again after a change.

```python
for item in pyst.children():
    info = pyst.info(item)
    print(info["name"], info["kind"])
```

### Describe an object

`pyst.info(handle)` returns a dictionary with these keys:

| Key | Value |
|---|---|
| `name` | The name of the object. For the root: the project name |
| `kind` | `"folder"`, `"workbook"`, `"worksheet"`, `"column"`, `"graph"`, `"matrix"`, `"note"` or `"unknown"` |
| `parent` | The handle of the parent, or `None` for the root |
| `row_count` | For a column: its number of rows. For a sheet: the number of rows of its longest column. Otherwise `None` |
| `column` | For a column: the dictionary below. Otherwise `None` |

For a column, `info["column"]` has these keys:

| Key | Value |
|---|---|
| `type` | `"numeric"`, `"datetime"` or `"mixed"` |
| `role` | `"x"`, `"y"`, `"x2"`, `"y2"`, `"x3"`, `"y3"`, `"x4"`, `"y4"`, `"z"`, `"x_error"`, `"y_error"`, `"label"` or `"none"` |
| `long_name` | The long name |
| `unit` | The unit |
| `comment` | The comment |
| `x_prime` | A number or `None` (read-only) |
| `display_format` | The display format, as text |
| `formula` | The formula in column letters, or `None` |
| `version` | A counter that increases each time the column data changes |

The dictionary is a copy. Changing it changes nothing in the project. Call `pyst.info` again to see the current state.

```python
sheet = pyst.resolve("[Book1]Sheet1")
for column in pyst.children(sheet):
    info = pyst.info(column)
    print(info["name"], info["column"]["unit"], info["row_count"])
```

## Read data

### Numeric columns as numpy arrays

`pyst.column(handle)` returns the values of a numeric or date column as a numpy array of `float64`. The array shares the memory of the column: nothing is copied, so it is fast on large columns. In exchange, the array is **read-only**. To change values, make a new array (`data * 2`, `data.copy()`) and write it back.

Empty cells are `nan`. Dates are numbers: seconds since 1 January 1970 (POSIX time).

`pyst.column` refuses a column of type `mixed` with `InvalidValueError`. Use `pyst.values` for those.

```python
import numpy as np

data = pyst.column(pyst.resolve("[Book1]Sheet1!B"))
print(len(data), np.nanmin(data), np.nanmax(data))
```

### Any column as a tuple

`pyst.values(handle)` returns a tuple with one item per row. This works on every type of column. It copies the data, so use `pyst.column` for large numeric columns.

| Cell | Python value |
|---|---|
| Number | `float` |
| Text | `str` (a text such as `"12.5"` stays text) |
| Empty cell | `None` |
| Empty text | `""` |
| Not a number | `float("nan")` |

```python
labels = pyst.values(pyst.resolve("[Book1]Sheet1!Label"))
```

## Write data

### Replace the values of a column

`pyst.set_values(handle, values)` replaces all the values of a column. The new list can have a different length. The type, name and metadata of the column do not change.

`values` can be:

- a numpy array of `float64` with one dimension;
- a list, tuple or other iterable of `float`, `int`, `str` or `None`.

Rules:

- A numeric or date column accepts numbers. `None` becomes an empty cell. Text is refused.
- A `mixed` column keeps every item as it is, text included.
- `True` and `False` are refused. Convert them with `int(...)` first.
- Integers larger than 2^53 in absolute value are refused, because they cannot be stored exactly.
- A column that is filled by a formula or a calculation cannot be written.

```python
c = pyst.resolve("[Book1]Sheet1!C")
pyst.set_values(c, [1.0, 2.5, None, 4])
```

`pyst.set_column(handle, values)` is the strict version. It takes a numpy `float64` array of **the same length** as the column, and refuses anything else.

```python
b = pyst.resolve("[Book1]Sheet1!B")
pyst.set_column(b, pyst.column(b) * 2)
```

### Protection against lost edits

When a script reads a column with `pyst.column` or `pyst.values`, StokesForge remembers the version it read. If the column changes before the script writes it back (for example because you edited a cell by hand), the write is refused with `ConcurrentWriteError`, so the script never overwrites your edit.

After a successful write, the script can write the same column again.

You can give the version yourself with the argument `expected_version`, taken from `pyst.info(handle)["column"]["version"]`:

```python
version = pyst.info(b)["column"]["version"]
pyst.set_column(b, new_data, expected_version=version)
```

### Change the properties of a column

`pyst.set_column_meta(handle, **fields)` changes only the properties you name. The other properties keep their values. The fields are:

| Field | Value |
|---|---|
| `role` | One of the roles listed under `pyst.info` |
| `long_name` | Text |
| `unit` | Text |
| `comment` | Text |
| `display_format` | Text |
| `formula` | Text in column letters (for example `"A * 2"`), or `None` to remove the formula |

An unknown field, a role that does not exist, or a formula that refers to a missing column raises `InvalidValueError`, and nothing is changed.

```python
pyst.set_column_meta(b, role="y", long_name="Absorbance", unit="a.u.")
pyst.set_column_meta(c, formula="B * 2")
```

A formula column is calculated by StokesForge. You cannot write its values.

`pyst.set_column_type(column, column_type)` converts a column to `"numeric"`, `"datetime"` or `"mixed"`. Values that the new type cannot hold are lost. A formula column refuses the conversion.

### Rows

`pyst.insert_rows(sheet, at, count=1)` inserts empty rows at row number `at` (0 is the top) in all the columns of the sheet. `at` cannot be beyond the last row.

`pyst.delete_rows(sheet, at, count=1)` deletes `count` rows from row `at` in all the columns of the sheet.

```python
sheet = pyst.resolve("[Book1]Sheet1")
pyst.insert_rows(sheet, 0, 2)    # two empty rows at the top
pyst.delete_rows(sheet, 10, 5)   # remove rows 10 to 14
```

## Create and organise objects

### Workbooks, sheets and columns

`pyst.create_workbook(name=None, parent=None)` creates a workbook with **no sheet** and returns its handle. Without `name`, StokesForge picks the first free name (Book1, Book2…). `parent` is a folder handle. Without it, the workbook goes to the root.

`pyst.create_worksheet(workbook, name=None)` adds an empty sheet at the end of the workbook. Without `name`, StokesForge uses Sheet1, Sheet2…

`pyst.create_column(worksheet, name=None, column_type="numeric")` adds an empty column at the end of the sheet. Without `name`, StokesForge uses A, B, C… The type is `"numeric"`, `"datetime"` or `"mixed"`. Use `"mixed"` for text.

A new column has no role. Set the roles with `pyst.set_column_meta` if you want X and Y columns.

```python
book = pyst.create_workbook("Results")
sheet = pyst.create_worksheet(book, "Data")
x = pyst.create_column(sheet, "Time")
y = pyst.create_column(sheet, "Signal")
names = pyst.create_column(sheet, "Sample", column_type="mixed")

pyst.set_values(x, [0.0, 1.0, 2.0])
pyst.set_values(y, [0.1, 0.4, 0.9])
pyst.set_values(names, ["a", "b", "c"])
pyst.set_column_meta(x, role="x", unit="s")
pyst.set_column_meta(y, role="y")
```

Names follow the rules of the interface: at most 255 characters, no reserved characters, and unique among their siblings. A name that is refused raises `InvalidValueError`.

### Rename, move, copy and delete

| Call | Effect |
|---|---|
| `pyst.rename(handle, name)` | Renames the object. The handle stays valid. |
| `pyst.move(handle, parent, index=None)` | Moves the object under `parent`. `index` is the position among the other children; without it, the object goes last. With the same parent, it reorders. A parent that cannot hold the object, a name already taken or an index out of range raises `InvalidValueError`. |
| `pyst.duplicate(handle, name=None)` | Copies the object with everything it contains, in the same parent, and returns the handle of the copy. A sheet is copied right after the original. Without `name`, the copy is named "*name* copy *N*". The root cannot be copied. |
| `pyst.remove(handle)` | Deletes the object and everything it contains. The root cannot be deleted. |

### Notes

`pyst.set_note(note, text)` replaces the whole text of a note. `note` is the handle of a note.

## Import files

`pyst.import_file(path, recipe=None)` imports an instrument file into a **new workbook** and returns the list of handles of the sheets it created.

- `path` is a text or a `pathlib.Path`.
- Without `recipe`, StokesForge chooses an import template automatically, as the import window does. With `recipe`, give the name of a template you saved in the import window.
- The import applies the template: ignored columns, column types, metadata, header blocks and the split into several sheets.
- If the file cannot be read, or the template does not exist, `InvalidValueError` is raised and the project is not changed.
- A single undo removes the whole import.

```python
sheets = pyst.import_file("/home/me/data/spectrum.csv")
sheet = sheets[0]
print(pyst.info(sheet)["name"], pyst.info(sheet)["row_count"])
```

To learn about templates and what the import does, see [Importing data](../guide/import.md).

## Create and edit graphs

### Plot columns

`pyst.plot(y, x=None, *, graph=None, name=None, parent=None, **options)` creates a graph from columns and returns its handle.

- `y` is a column handle, or a list of column handles. A list creates one series per column, grouped together.
- `x` is the column for the horizontal axis. Without it, each series uses the nearest X column to the left of its Y column. If the sheet has no X column, the row number is used. `x` can also be a list with one handle per series.
- `name` names the graph. Without it, StokesForge uses Graph1, Graph2…
- `parent` is the folder that will hold the graph.
- `graph` adds the series to an existing graph instead of creating one. With `graph`, `name` and `parent` are not allowed.

The options are:

| Option | Values |
|---|---|
| `plot_type` | `"line"`, `"scatter"`, `"line_scatter"`, `"bar"`, `"histogram"`, `"error_bar"` |
| `color` | `"#RRGGBB"` or `"#RRGGBBAA"`. A list gives one color per series. Without it, the palette is used. |
| `line_style` | `"none"`, `"solid"`, `"dash"`, `"dot"`, `"dash_dot"` |
| `line_width` | A positive number |
| `symbol` | `"none"`, `"circle"`, `"square"`, `"triangle"`, `"diamond"`, `"plus"`, `"cross"` |
| `symbol_size` | A positive number |
| `label` | The legend text. A list gives one text per series. |
| `group_name` | The name of the group, when `y` is a list |
| `title`, `x_title`, `y_title` | Titles. They accept the same formatting as the graph properties. |
| `x_scale`, `y_scale` | `"linear"` or `"log"` |
| `x_from`, `x_to`, `y_from`, `y_to` | Axis range. Give both bounds of an axis together. A reversed range (`from` greater than `to`) reverses the axis. |
| `axis` | `"left"` or `"right"`: which Y axis the series uses |
| `x_error`, `y_error` | Handle of an error column (one series only) |
| `histogram_bins` | Number of bins, from 1 to 65536 |

An unknown option, an invalid range or a wrong target raises `InvalidValueError`.

```python
x = pyst.resolve("[Book1]Sheet1!A")
y = pyst.resolve("[Book1]Sheet1!B")

g = pyst.plot(y, x, name="Spectrum", plot_type="line",
              x_title="Wavelength (nm)", y_title="Absorbance")

# Add a second series to the same graph.
y2 = pyst.resolve("[Book1]Sheet1!C")
pyst.plot(y2, x, graph=g, line_style="dash", label="Blank")
```

### Change graph properties

`pyst.set_graph(graph, *, panel=(0, 0), axis="left", **options)` changes properties of an existing graph. All the options of one call form a single history entry. If you give a wrong value, nothing is changed.

`panel` is the (row, column) of the panel, `(0, 0)` for a graph with a single panel. The Y options apply to the Y axis named by `axis`.

| Option | Value |
|---|---|
| `title`, `panel_title`, `x_title`, `y_title`, `x_unit`, `y_unit` | Text |
| `title_visible`, `stale_warning`, `decimal_comma` | `True` or `False` |
| `title_color`, `background` | `"#RRGGBB"` or `"#RRGGBBAA"`. An empty text means automatic (title) or white (background). |
| `x_scale`, `y_scale` | `"linear"` or `"log"` |
| `x_from`, `x_to`, `y_from`, `y_to` | Axis range, both bounds together. The axis then has a fixed range. |
| `x_auto_range`, `y_auto_range` | `True` returns the axis to automatic range |
| `x_inverted`, `y_inverted` | `True` or `False`: reverses the axis |
| `x_grid`, `y_grid`, `x_visible`, `y_visible` | `True` or `False` |
| `legend_visible` | `True` or `False` |
| `legend_position` | `"top_left"`, `"top_right"`, `"bottom_left"`, `"bottom_right"` |

```python
pyst.set_graph(g, title="Absorption spectrum", x_from=300, x_to=800,
               legend_position="top_right")
```

### Add shapes

`pyst.add_shape(graph, kind, x1, y1, x2, y2, *, panel=(0, 0), axis="left", line_color="", line_width=1.5, fill_color="")` draws a shape in the graph and returns its rank in the panel. `kind` is `"rectangle"`, `"ellipse"`, `"line"` or `"arrow"` (the arrowhead is at `(x2, y2)`). Coordinates are in data units. An empty `line_color` is automatic; an empty `fill_color` means no fill.

`pyst.remove_shape(graph, index, *, panel=(0, 0))` removes the shape with this rank. The shapes after it move down by one.

```python
pyst.add_shape(g, "arrow", 500, 0.2, 520, 0.8, line_color="#aa3311")
```

### Series and groups

`pyst.series(graph, *, panel=(0, 0))` returns a list with one dictionary per series, with the keys `index`, `name`, `group` (`None` if the series is alone), `color`, `visible` and `y` (the handle of its Y column).

When you plot several columns together, the series form a group. These functions edit groups. `group` is the group number from `pyst.series_groups`.

| Call | Effect |
|---|---|
| `pyst.series_groups(graph, *, panel=(0, 0))` | List of dictionaries with `id`, `name` (`None` for the automatic name) and `members` (series ranks) |
| `pyst.group_series(graph, indices, *, name=None, panel=(0, 0))` | Groups the series with these ranks. Returns the group number. |
| `pyst.set_series_group(graph, index, group, *, panel=(0, 0))` | Moves a series into a group, or out of its group with `None`. Returns its new rank. |
| `pyst.rename_group(graph, group, name, *, panel=(0, 0))` | Renames a group. An empty name restores the automatic name. |
| `pyst.ungroup(graph, group, *, panel=(0, 0))` | Dissolves the group. The series stay and keep their style. |
| `pyst.remove_group(graph, group, *, panel=(0, 0))` | Removes the group **and its series** |
| `pyst.set_group_style(graph, group, *, panel=(0, 0), line_style=None, line_width=None, symbol=None, symbol_size=None)` | Sets each given option on all the members |
| `pyst.apply_gradient(graph, group, gradient, *, panel=(0, 0))` | Colors the members along a gradient. `gradient` is the name of a built-in palette, the name of a palette saved in the project, or a list of `"#RRGGBB"` colors. |

Palettes saved in your user settings are not visible to scripts, so that a script gives the same result on every computer.

### Export a graph

`pyst.export_graph(graph, path, *, png_scale=4.0)` saves a graph as an image. The file type comes from the extension of `path`: `.svg`, `.pdf` or `.png`. Any other extension raises `InvalidValueError`. `png_scale` is the size factor for PNG files. The result is the same as the export from the application.

```python
pyst.export_graph(g, "/home/me/figures/spectrum.pdf")
```

## Analyse data

The `pyst.analysis` module runs the same calculations as the **Analysis** menu. For the meaning of the settings, see [Analysis](../guide/analysis.md).

Each function:

- takes the handle of a Y column `y` and, optionally, the handle of its X column `x`;
- records the calculation in the project, with its result columns, as the menu does;
- is part of the run's undo step;
- returns a dictionary with four keys.

| Key | Content |
|---|---|
| `columns` | Dictionary from result name to the handle of the column written, for example `"corrected"` or `"baseline"` |
| `values` | Dictionary of numbers, for example `"area.total"`, `"param.tau"` or `"peak.height.1"` |
| `errors` | The standard error of the values that have one, with the same keys as `values` |
| `warnings` | List of English sentences about the quality of the result |

`y` can also be a list of handles. The calculation is then done on all the series in one step, and the result is `{"series": [one dictionary per series, in the same order]}`. `x` is then one handle shared by all the series, or a list with one handle per series.

All the functions accept `keep_results` and `auto_recalc`. `auto_recalc=True` recalculates the result when the source data changes, as in the analysis window.

### Functions

| Function | What it does |
|---|---|
| `pyst.analysis.baseline(y, x=None, *, method="anchors", points=None, zones=None, degree=1, interpolation="segments", op="subtract", keep_baseline=False, keep_results=False, auto_recalc=True, replace_original=False)` | Estimates and removes a baseline |
| `pyst.analysis.correct(y, reference, x=None, reference_x=None, *, op="divide", keep_results=False, auto_recalc=True, replace_original=False)` | Corrects a curve with a reference curve (`op`: `"divide"`, `"multiply"` or `"subtract"`) |
| `pyst.analysis.smooth(y, x=None, *, method="adjacent", window=5.0, window_unit="points", order=2, keep_results=False, auto_recalc=True, replace_original=False)` | Smooths a curve |
| `pyst.analysis.integrate(y, x=None, *, ranges=None, under="zero", cumulative=False, normalize=False, keep_results=False, auto_recalc=True)` | Integrates a curve |
| `pyst.analysis.fit(y, x=None, *, model="linear", degree=3, initial=None, bounds=None, fixed=None, shared=None, fit_range=None, residuals=False, sigma=None, keep_results=False, auto_recalc=True)` | Fits a model |
| `pyst.analysis.peaks(y, x=None, *, shape="gaussian", background="constant", centers=None, link_widths=False, center_window=None, prominence=0.05, max_peaks=6, fit_range=None, residuals=False, peak_columns=True, sum_column=True, keep_results=False, auto_recalc=True)` | Fits several peaks |
| `pyst.analysis.reconvolve(y, response, x=None, response_x=None, *, components=1, shift=0.0, fit_shift=True, window=None, residuals=False, keep_results=False, auto_recalc=False)` | Fits exponentials convolved with an instrument response |
| `pyst.analysis.deconvolve(y, response, x=None, response_x=None, *, method="richardson-lucy", iterations=50, noise=None, shift=0.0, keep_results=False, auto_recalc=False)` | Removes the instrument response by deconvolution |
| `pyst.analysis.readiness(kind, **options)` | Tells whether a calculation has what it needs, without running it |

### Baseline

`method` is `"anchors"`, `"constant"`, `"line"` or `"zones"`.

| Method | What you give |
|---|---|
| `"anchors"` | `points=[(x, y), ...]`: points the baseline passes through. `interpolation` chooses how it joins them. |
| `"constant"` | Exactly one of `points=[(x, y)]` (one point) or `zones=[(x_min, x_max)]` (the mean of the Y values in the zone) |
| `"line"` | `points`: two points |
| `"zones"` | `zones=[(x_min, x_max), ...]`: regions without signal, and `degree` for the polynomial fitted through them |

`op` is `"subtract"` (default) or `"divide"`. The corrected curve is in `columns["corrected"]`, and the baseline in `columns["baseline"]`. The baseline column is kept in the project when `keep_baseline=True`.

With `replace_original=True`, the result is written into the source Y column, instead of a new column. `columns["corrected"]` is then the source column. The result is not recalculated later and cannot be edited in the analysis window. One undo reverts the whole run.

```python
x = pyst.resolve("[Book1]Sheet1!A")
y = pyst.resolve("[Book1]Sheet1!B")
r = pyst.analysis.baseline(y, x, method="constant", zones=[(780.0, 800.0)])
corrected = r["columns"]["corrected"]
```

### Smooth

`method` is `"adjacent"` (adjacent-point average), `"savitzky-golay"` or `"median"`. `window` is an odd number of points, or a width in X units when `window_unit="x"`. `order` (2 to 5) applies to Savitzky-Golay. The result is in `columns["smoothed"]`.

### Integrate

`ranges=[(x_min, x_max), ...]` gives the intervals. Without `ranges`, the whole curve is integrated. `under` is `"zero"` (down to Y = 0) or `"line"` (down to the straight line between the interval ends). The total area is in `values["area.total"]`.

### Fit

`model` is one of `"linear"`, `"polynomial"` (with `degree`), `"exponential"`, `"exponential_offset"`, `"exponential2"`, `"logarithm"`, `"power"`, `"gaussian"`, `"lorentzian"`, or a formula in the variable `x`, such as `"a*exp(-x/b)+c"`.

- `initial` gives starting values: `{"xc": 520.0, "fwhm": 12.0}`.
- `bounds` limits parameters: `{"tau": (0.0, 100.0)}`.
- `fixed` is a list of parameter names that stay at their starting value.
- `shared` is a list of parameter names shared by all the series of a batch (global fit).
- `fit_range` restricts the fit to an X interval.
- `sigma` gives weights.
- `residuals=True` also writes the residuals.

The fitted parameters are in `values` with the prefix `param.`, for example `values["param.tau"]`, with their errors in `errors`. The names depend on the model: `intercept` and `slope` for a line, `A` and `tau` for an exponential, `y0`, `A`, `xc` and `fwhm` for a gaussian.

```python
r = pyst.analysis.fit(y, x, model="exponential")
print(r["values"]["param.tau"], "+/-", r["errors"]["param.tau"])
```

### Peaks

Without `centers`, the peaks are detected, using `prominence` (a fraction of the range of the data) and `max_peaks`. With `centers=[...]`, you place the peaks yourself. `center_window` limits how far each center can move from its starting position. The peak values are in `values`, with keys such as `peak.height.1`.

### Check before you run

`pyst.analysis.readiness(kind, **options)` says whether a calculation can start, without reading data or changing anything. `kind` is `"baseline"`, `"integrate"` or `"fit"`, with the same options as the function of the same name. The result has `ready` (`True` or `False`), `reason` (`None` when ready; otherwise `"needs_points"`, `"needs_zone"`, `"bad_expression"`, `"no_parameter"` or `"invalid"`) and `message` in English.

### scipy

scipy is available in scripts. Use it for a model that `pyst.analysis.fit` does not offer, then write the result with `pyst.set_values`.

```python
import numpy as np
from scipy.optimize import curve_fit

x = pyst.column(pyst.resolve("[Book1]Sheet1!A"))
y = pyst.column(pyst.resolve("[Book1]Sheet1!B"))

def model(x, a, k):
    return a * np.exp(-k * x)

params, _ = curve_fit(model, x, y, p0=(1.0, 0.1))
print(params)
```

## Undo and redo

`pyst.undo()` undoes the changes made by the script so far, and returns `True`. If there is nothing to undo, it returns `False`. After `pyst.undo()`, the next change starts a new group.

`pyst.redo()` redoes what was undone and returns `True`, or `False` if there is nothing to redo.

If the script writes something after `pyst.undo()`, the undone changes can no longer be redone.

## Save

`pyst.save()` saves the project to its current file. If the project has no file yet, it raises `InvalidValueError`; use `pyst.save_as`.

`pyst.save_as(path)` saves the project to `path` (a text or a `pathlib.Path`). This path becomes the current file of the project.

In the console, the project keeps the file it was opened from, so `pyst.save()` works on a saved project. In a terminal run with `stokesforge-cli`, the project has no file until you call `pyst.save_as`. Saving does not add an undo step.

## Worked examples

### Normalise a spectrum

This script reads the spectrum in columns A (wavelength) and B (absorbance) of **Book1**, divides it by its maximum, stores the result in a new workbook, and plots it.

```python
import numpy as np

x_source = pyst.resolve("[Book1]Sheet1!A")
y_source = pyst.resolve("[Book1]Sheet1!B")

wavelength = pyst.column(x_source)
absorbance = pyst.column(y_source)
normalised = absorbance / np.nanmax(absorbance)

book = pyst.create_workbook("Normalised")
sheet = pyst.create_worksheet(book, "Data")
x = pyst.create_column(sheet, "Wavelength")
y = pyst.create_column(sheet, "Absorbance")

pyst.set_values(x, wavelength)
pyst.set_values(y, normalised)
pyst.set_column_meta(x, role="x", unit="nm")
pyst.set_column_meta(y, role="y", long_name="Normalised absorbance")

pyst.plot(y, x, name="Normalised plot", plot_type="line",
          x_title="Wavelength (nm)", y_title="Normalised absorbance")
```

One <kbd>Ctrl</kbd>+<kbd>Z</kbd> removes the workbook and the graph.

### Import a folder of files

This script imports every `.csv` file of a folder, one workbook per file, and saves the project.

```python
from pathlib import Path

folder = Path("/home/me/data/spectra")

for file in sorted(folder.glob("*.csv")):
    sheets = pyst.import_file(file)
    book = pyst.info(sheets[0])["parent"]
    print(file.name, "->", pyst.info(book)["name"])

pyst.save_as(folder / "spectra.stf")
```

To run it on a server without a window:

```sh
stokesforge-cli run import_all.pyst
```

### Remove a baseline and measure a peak area

This script subtracts a constant baseline measured on a flat region, integrates the peak, and prints the area.

```python
x = pyst.resolve("[Book1]Sheet1!A")
y = pyst.resolve("[Book1]Sheet1!B")

base = pyst.analysis.baseline(y, x, method="constant",
                              zones=[(780.0, 800.0)])
corrected = base["columns"]["corrected"]

area = pyst.analysis.integrate(corrected, x, ranges=[(480.0, 560.0)])
print("Peak area:", area["values"]["area.total"])
for warning in area["warnings"]:
    print("Warning:", warning)
```

Both calculations appear in the project like the ones you start from the **Analysis** menu, and one undo removes them together.
