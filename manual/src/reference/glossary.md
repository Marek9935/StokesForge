# Glossary

This page defines the terms you meet in StokesForge, in alphabetical order. Each entry is short and links to the page that explains the subject.

## A

**Active layer**. The layer of a graph that receives what you add: dropped columns, **Send to**, text and new layers. Click a layer badge or the plot area of a layer to make it active. See [The active layer](../guide/graphs.md#the-active-layer).

**Analysis**. The menu and the window for mathematical processing: corrections, smoothing, integration, fits, peak decomposition and deconvolution. See [Analysis](../guide/analysis.md).

**Anchor point**. A point you place on the preview of an analysis to draw a baseline. See [Baseline correction](../guide/analysis.md#baseline-correction).

## B

**Baseline**. The background signal under the peaks. A baseline correction computes a baseline and subtracts it from the data, or divides the data by it. See [Baseline correction](../guide/analysis.md#baseline-correction).

**Block**. In an imported file, one series of data separated from the others, for example one spectrum among several in the same file. See [Blocks and encoding](../guide/import.md#blocks-and-encoding).

## C

**Calculation**. A saved analysis, with its settings and its output columns. You can edit it, recalculate it or delete it. See [Editing, recalculating and undoing](../guide/analysis.md#editing-recalculating-and-undoing).

**Cell**. One value, at the crossing of a column and a row of a sheet. See [Cells](../guide/projects-and-workbooks.md#cells).

**Column**. A series of values of one type in a sheet. A column has a letter, a type, a role and a header. See [Columns](../guide/projects-and-workbooks.md#columns).

**Column type**. The kind of values a column holds: **Numeric**, **Text**, **Mixed** or **Date/Time**. A **Mixed** column holds numbers, text and empty cells, each cell by itself. See [Column type](../guide/projects-and-workbooks.md#column-type).

**Comments**. A free text attached to a column, in the header. See [The column header](../guide/projects-and-workbooks.md#the-column-header).

**Configuration folder**. The folder where StokesForge keeps your preferences, import templates and saved colors. See [Where your settings are kept](../getting-started/installation.md#where-your-settings-are-kept).

**Correction**. An analysis type that computes a baseline (constant, line, polynomial or anchor points) and subtracts it from the data or divides the data by it. Not to be confused with **Reference**. See [Baseline correction](../guide/analysis.md#baseline-correction).

## D

**Data end**. The place where the data of an imported file stop. The lines after it form the footer. See [Data end](../guide/import.md#data-end).

**Deconvolution**. An analysis that recovers the signal before the instrument response. See [Direct deconvolution](../guide/analysis.md#direct-deconvolution).

**Delimiter**. The character that separates the values on a line of an imported file. See [Delimiter](../guide/import.md#delimiter).

**Detached window**. A workbook, graph, note or panel that you moved out of the main window, into a window of its own, for example onto a second screen. See [Windows in the workspace](../guide/interface.md#windows-in-the-workspace).

## F

**F(x)=**. The header row of a column where you type a formula. See [Columns computed by a formula](../guide/projects-and-workbooks.md#columns-computed-by-a-formula).

**Fit**. To make a model match the data by least squares. The fit gives the parameters of the model with their standard errors. See [Fitting](../guide/analysis.md#fitting).

**Folder**. A container of workbooks, graphs, notes and other folders in the project. See [The Project Manager and folders](../guide/projects-and-workbooks.md#the-project-manager-and-folders).

**Formula**. An expression typed in the **F(x)=** row that computes the values of a column from other columns of the same sheet. The values of the column are read-only. See [Columns computed by a formula](../guide/projects-and-workbooks.md#columns-computed-by-a-formula).

**FWHM**. Full width at half maximum: the width of a peak at half of its height. See [Peak decomposition](../guide/analysis.md#peak-decomposition).

## G

**Graph**. A window that plots your data as a figure. A graph holds one or more layers. See [Graphs](../guide/graphs.md).

**Graph Properties**. The window that holds the settings of a graph, in the tabs **General**, **Plot Area**, **Axes**, **Legend** and **Theme**. See [Open the graph settings](../guide/graphs.md#open-the-graph-settings).

## H

**Header** (column). The rows above the data of a column: letter and role, **Long Name**, **Units**, **Comments** and **F(x)=**. See [The column header](../guide/projects-and-workbooks.md#the-column-header).

**Header** (import). The lines at the start of an imported file that are not data. See [Step 3: header and data end](../guide/import.md#step-3-header-and-data-end).

## I

**Import Wizard**. The window with six steps that sets up the import of text files. See [The wizard at a glance](../guide/import.md#the-wizard-at-a-glance).

**Instrument response (IRF)**. The response of the instrument to a very short signal. Reconvolution and direct deconvolution use it. See [Reconvolution](../guide/analysis.md#reconvolution).

## L

**Layer**. One plot area of a graph, with its axes. A graph has one layer to start with and can have up to three overlay layers. See [Overlay layers](../guide/graphs.md#overlay-layers).

**Legend**. The list of the series of a graph, each with its style. See [Legend](../guide/graphs.md#legend).

**Long Name**. A description of a column, shown in its header, for example `Wavelength`. See [The column header](../guide/projects-and-workbooks.md#the-column-header).

## M

**Metadata**. Pairs of a key and a value read from the header or footer of an imported file, such as the date or the instrument. They are kept with each imported column. See [Step 5: metadata](../guide/import.md#step-5-metadata).

## N

**Note**. A window of free text in the project. See [Windows in the workspace](../guide/interface.md#windows-in-the-workspace).

## O

**Object Manager**. The panel at the right that lists the content of the active graph: layers, series groups, series, axes and free objects. See [The Object Manager](../guide/graphs.md#the-object-manager).

**Out of date**. The state of a calculation whose source data changed or disappeared. Its columns are marked until you recalculate. See [Out-of-date results](../guide/analysis.md#out-of-date-results).

**Overlay layer**. A layer drawn on top of the first one, with its own axes. See [Overlay layers](../guide/graphs.md#overlay-layers).

## P

**Peak decomposition**. An analysis that fits a sum of peaks and a background to a curve. See [Peak decomposition](../guide/analysis.md#peak-decomposition).

**Plot type**. The way a series is drawn: **Scatter**, **Line**, **Line + Symbol**, **Bar**, **Histogram** or **Line + Error Bars**. See [Change a series](../guide/graphs.md#change-a-series).

**Preferences**. The settings of the application, not of one project. Open them with **File ▸ Preferences…**. See [Preferences](../guide/interface.md#preferences).

**Project**. Everything you work on: folders, workbooks, graphs, notes and analyses. A project is saved in one `.stf` file. See [Projects](../guide/projects-and-workbooks.md#projects).

**Project Manager**. The panel at the left that shows the tree of the folders and windows of the project. See [Project Manager](../guide/interface.md#project-manager).

**pyst**. The Python module that gives scripts access to your project. It is already loaded in the script console. See [The pyst API](../scripting/api.md).

**Python Script panel**. The panel at the bottom of the window where you write and run scripts. Also called the script console. See [The script console](../scripting/console.md).

## R

**R²**. The coefficient of determination of a fit: how well the model describes the data. A value below 0.6 is reported as a low R². See [Goodness of fit](../guide/analysis.md#goodness-of-fit).

**Recalculate**. To redo a calculation on the current data. See [Editing, recalculating and undoing](../guide/analysis.md#editing-recalculating-and-undoing).

**Recovery copy**. A copy of the open project that StokesForge writes every few minutes in a separate folder. It is offered back after a crash and never replaces your file. See [Recovery copies](../guide/projects-and-workbooks.md#recovery-copies).

**Reference**. An analysis type that corrects a curve with a reference column measured separately, such as a blank, by dividing, multiplying or subtracting. Not to be confused with **Correction**. See [Reference correction](../guide/analysis.md#reference-correction).

**Results**. The pane under the sheet that lists the calculations you kept, with their values. See [The Results pane](../guide/analysis.md#the-results-pane).

**Role**. What a column is used for in a plot: **X**, **Y**, **Z**, **X Error**, **Y Error**, **Label** or **None**, and the roles of the extra layers. It is written after the letter of the column, for example `B(Y)`. See [Column role](../guide/projects-and-workbooks.md#column-role).

## S

**Series**. One curve of a graph: a Y column drawn against an X column. See [How columns become series](../guide/graphs.md#how-columns-become-series).

**Series group**. A set of series added together, which you can style as a whole. See [Series groups](../guide/graphs.md#series-groups).

**Sheet**. A grid of columns inside a workbook. A workbook holds one or more sheets. See [Sheets](../guide/projects-and-workbooks.md#sheets).

**Smoothing**. An analysis that reduces the noise of a curve from its neighbouring points: adjacent averaging, Savitzky-Golay or rolling median. See [Smoothing](../guide/analysis.md#smoothing).

**`.stf`**. The file extension of a StokesForge project. See [Project files](project-files.md).

**Status bar**. The bottom line of the window. It shows the last message and the progress of long work. See [Status bar](../guide/interface.md#status-bar).

**Subheader**. The lines at the end of the header of an imported file that give the name, the units or the comments of each column. See [Subheader lines](../guide/import.md#subheader-lines).

## T

**Template** (import). The settings of the Import Wizard, saved under a name and reused for the files of one instrument. A template can choose itself for a file. See [Templates](../guide/import.md#templates).

**Theme**. A set of global colors of a graph, **Light** or **Dark**. See [The tabs](../guide/graphs.md#the-tabs).

**Transposed sheet**. A sheet created from the rows of another sheet, named like it with a `ᵀ` suffix, so that you can analyse the rows. See [Linked sheets](../guide/projects-and-workbooks.md#linked-sheets).

**Typst**. The markup language used for titles, legends and texts of a graph. Text between dollar signs is math. See [Titles and formulas](../guide/graphs.md#titles-and-formulas).

## U

**Units**. The unit of the values of a column, shown in its header, for example `nm`. See [The column header](../guide/projects-and-workbooks.md#the-column-header).

## W

**Workbook**. A data window that contains one or more sheets. See [Projects and workbooks](../guide/projects-and-workbooks.md).

**Workspace**. The central zone of the main window, where your workbooks, graphs and notes open. See [The zones of the window](../guide/interface.md#the-zones-of-the-window).

## X

**x' row**. A header row that holds one number for each column, such as the time of a measurement. It lets you plot and analyse rows. Show it with **Workbook ▸ Show x' Row**. See [Header rows](../guide/projects-and-workbooks.md#header-rows).
