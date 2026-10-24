// The component kit (docs/DESIGN.md §5). Screens import from here.
import "./ui.css";
import "./plot/plot.css";
import "./maps/maps.css";

export { Button, IconButton } from "./Button";
export { Card, Section, Advanced } from "./Card";
export { StatusPill, type PillTone } from "./StatusPill";

export { NumberField } from "./NumberField";
export { Select, Stepper, Toggle, Segmented, type Option } from "./controls";
export { DataTable, type Column, type RowGroup } from "./Table";
export { LevelMeter, ProgressBar } from "./meters";
export { ToastView } from "./Toast";
export { Dialog, ConfirmDialog, useConfirmAndUndo } from "./Dialog";
export { Tooltip } from "./Tooltip";
export { HelpTip } from "./HelpTip";
export { Menu, type MenuItem } from "./Popover";
export { EmptyState, ErrorNotice } from "./states";
export { Icon, Spinner } from "./Icon";


export { LogPlot, type PlotScale } from "./plot/LogPlot";
export { Graph } from "./plot/Graph";
export { type LegendChannel } from "./plot/Legend";
export { AxisRangeButton, type AxisRangeValue } from "./plot/AxisRange";
export { TargetHandles, XoLine } from "./plot/editors";
export { type Curve } from "./plot/series";
export { formatHz } from "./plot/ticks";
export * as target from "./plot/targetEdit";

export { RoomMap } from "./maps/RoomMap";
export { ArrangementMap, ArrangementThumb } from "./maps/ArrangementMap";
export * as geometry from "./maps/geometry";
export { CellSelect } from "./CellSelect";
