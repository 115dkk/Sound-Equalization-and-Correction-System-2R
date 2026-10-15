// The component kit (docs/DESIGN.md §5). Screens import from here.
import "./ui.css";
import "./plot/plot.css";
import "./maps/maps.css";

export { Button, IconButton, type ButtonProps, type ButtonVariant } from "./Button";
export { Card, Section, Advanced } from "./Card";
export { StatusPill, type PillTone } from "./StatusPill";
export { Field } from "./Field";
export { NumberField, checkNumber, stepNumber, type NumberFieldProps } from "./NumberField";
export { Select, Stepper, Toggle, Segmented, type Option } from "./controls";
export { DataTable, type Column, type RowGroup } from "./Table";
export { LevelMeter, ProgressBar, levelState, METER_BAND } from "./meters";
export { ToastProvider, ToastView, useToast, type ToastInput } from "./Toast";
export { Dialog, ConfirmDialog, useConfirmAndUndo } from "./Dialog";
export { Tooltip, TOOLTIP_DELAY_MS } from "./Tooltip";
export { Popover, Menu, type MenuItem } from "./Popover";
export { EmptyState, ErrorNotice } from "./states";
export { Icon, Spinner, type IconName } from "./Icon";
export { LayerRoot, Portal } from "./layer";

export { LogPlot, type PlotScale, type LogPlotProps } from "./plot/LogPlot";
export { Graph, type GraphProps } from "./plot/Graph";
export { Legend, toggled, type LegendChannel } from "./plot/Legend";
export { AxisRangeButton, validateRange, type AxisRangeValue } from "./plot/AxisRange";
export { TargetHandles, XoLine } from "./plot/editors";
export { type Curve, type Highlight } from "./plot/series";
export { FREQ_TICKS, FREQ_RANGE, formatHz } from "./plot/ticks";
export * as target from "./plot/targetEdit";

export { RoomMap, type RoomSpeaker } from "./maps/RoomMap";
export { ArrangementMap, ArrangementThumb, nextPosition } from "./maps/ArrangementMap";
export * as geometry from "./maps/geometry";
export { CellSelect } from "./CellSelect";
