// The component kit, one specimen per component with its states. The
// labels are real catalog strings where a screen will use the same words;
// sample values (names, numbers) are data, not copy.
import { useState, type ReactNode } from "react";
import { useT } from "../i18n/react";
import {
  Advanced,
  Button,
  Card,
  ConfirmDialog,
  DataTable,
  Dialog,
  EmptyState,
  ErrorNotice,
  IconButton,
  LevelMeter,
  NumberField,
  ProgressBar,
  Section,
  Segmented,
  Select,
  StatusPill,
  Stepper,
  Toggle,
  ToastView,
  Tooltip,
  useConfirmAndUndo,
} from "../ui";
import { roleCode } from "../ui/maps/geometry";
import type { SpecimenDef } from "./Specimen";

function Row({ children }: { children: ReactNode }) {
  return <div className="gal-row">{children}</div>;
}

function Buttons() {
  const t = useT();
  return (
    <div className="gal-stack">
      <Row>
        <Button variant="primary" icon="play" data-ui="gallery.button.primary">
          {t("frame.step.measure.primary")}
        </Button>
        <Button variant="secondary" icon="folder">
          {t("frame.project.open")}
        </Button>
        <Button variant="quiet" icon="undo">
          {t("ui.toast.undo")}
        </Button>
        <Button variant="danger">{t("gallery.sample.clear_position")}</Button>
      </Row>
      <Row>
        <Button variant="primary" busy>
          {t("frame.step.calibrate.primary")}
        </Button>
        <Button variant="primary" disabled>
          {t("frame.step.export.primary")}
        </Button>
        <Button variant="secondary" disabled>
          {t("frame.project.save")}
        </Button>
        <Button variant="secondary" size="sm" iconEnd="chevronDown">
          {t("frame.project")}
        </Button>
        <Tooltip content={t("plot.axes.tooltip")}>
          <IconButton icon="axes" size="sm" label={t("plot.axes.button")} />
        </Tooltip>
        <IconButton icon="close" label={t("ui.close")} />
      </Row>
    </div>
  );
}

function Cards() {
  const t = useT();
  return (
    <div className="gal-grid-2">
      <Card title={t("frame.status.positions")} description={t("frame.step.measure.purpose")} actions={<StatusPill tone="success">{t("frame.status.calibration.done")}</StatusPill>}>
        <p>{t("frame.placeholder.body")}</p>
      </Card>
      <Card title={t("plot.measurement.title")} stale actions={<StatusPill tone="warning">{t("frame.status.calibration.stale")}</StatusPill>}>
        <p className="muted">{t("error.stale_inputs.next")}</p>
      </Card>
      <Card title={t("gallery.sample.section_card")} dense>
        <Section title={t("gallery.sample.section_main")}>
          <NumberField id="gallery.section.rc_lo" label={t("gallery.sample.rc_lo")} value={20} onChange={() => {}} min={10} max={200} step={5} unit="Hz" />
          <Advanced id="gallery.section.advanced" count={2}>
            <NumberField id="gallery.section.boost" label={t("gallery.sample.boost")} value={10} onChange={() => {}} min={0} max={10} step={0.5} digits={1} unit="dB" />
          </Advanced>
        </Section>
        <Section title={t("gallery.sample.section_other")}>
          <Advanced id="gallery.section.advanced.open" defaultOpen>
            <NumberField id="gallery.section.boost2" label={t("gallery.sample.boost")} value={6} onChange={() => {}} min={0} max={10} step={0.5} digits={1} unit="dB" />
          </Advanced>
        </Section>
      </Card>
    </div>
  );
}

function Pills() {
  const t = useT();
  return (
    <Row>
      <StatusPill>{t("frame.status.calibration.none")}</StatusPill>
      <StatusPill tone="success">{t("frame.status.calibration.done")}</StatusPill>
      <StatusPill tone="warning">{t("frame.status.calibration.stale")}</StatusPill>
      <StatusPill tone="danger">{t("frame.status.calibration.failed")}</StatusPill>
      <StatusPill tone="info">{t("frame.status.path.rew")}</StatusPill>
      <StatusPill tone="busy">{t("frame.status.calibration.computing")}</StatusPill>
    </Row>
  );
}

function NumberFields() {
  const t = useT();
  const [lo, setLo] = useState(20);
  const [xo, setXo] = useState(80);
  return (
    <div className="gal-grid-4">
      <NumberField id="gallery.number.rc_lo" label={t("gallery.sample.rc_lo")} value={lo} onChange={setLo} min={10} max={200} step={5} unit="Hz" />
      <NumberField id="gallery.number.xo" label={t("plot.xo.label")} value={xo} onChange={setXo} min={40} max={200} step={5} unit="Hz" showRange />
      <InvalidNumber />
      <NumberField id="gallery.number.disabled" label={t("gallery.sample.boost")} value={10} onChange={() => {}} min={0} max={10} step={0.5} digits={1} unit="dB" disabled />
    </div>
  );
}

/** Shows the invalid state by typing an out-of-range value on mount. */
function InvalidNumber() {
  const t = useT();
  return (
    <div
      ref={(node) => {
        const input = node?.querySelector("input");
        if (input && input.dataset.typed !== "1") {
          input.dataset.typed = "1";
          // After React has attached its listeners: type 250 as a user would.
          window.setTimeout(() => {
            const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
            setter?.call(input, "250");
            input.dispatchEvent(new Event("input", { bubbles: true }));
          }, 0);
        }
      }}
    >
      <NumberField id="gallery.number.invalid" label={t("gallery.sample.rc_lo")} value={20} onChange={() => {}} min={10} max={200} step={5} unit="Hz" />
    </div>
  );
}

function Choices() {
  const t = useT();
  const [mode, setMode] = useState<"zero" | "mixed" | "linear">("mixed");
  const [subs, setSubs] = useState(2);
  const [expert, setExpert] = useState(false);
  const [theme, setTheme] = useState<"system" | "light" | "dark">("system");
  return (
    <div className="gal-grid-2">
      <div className="gal-stack">
        <Select
          id="gallery.select.mode"
          label={t("gallery.sample.mode")}
          value={mode}
          onChange={setMode}
          options={[
            { value: "zero", label: t("gallery.sample.mode.zero") },
            { value: "mixed", label: t("gallery.sample.mode.mixed") },
            { value: "linear", label: t("gallery.sample.mode.linear") },
          ]}
          hint={t("gallery.sample.mode.hint")}
        />
        <Select
          id="gallery.select.invalid"
          label={t("gallery.sample.output")}
          value="16"
          onChange={() => {}}
          options={[{ value: "16", label: t("gallery.sample.output.out_of_range", { n: 16 }) }]}
          error={t("gallery.sample.output.error", { max: 12 })}
        />
      </div>
      <div className="gal-stack">
        <Stepper id="gallery.stepper.subs" label={t("gallery.sample.subs")} value={subs} min={0} max={8} onChange={setSubs} />
        <Stepper id="gallery.stepper.min" label={t("gallery.sample.subs")} value={0} min={0} max={8} onChange={() => {}} />
        <Toggle id="gallery.toggle.expert" label={t("frame.settings.expert")} hint={t("frame.settings.expert.hint")} checked={expert} onChange={setExpert} />
        <Toggle id="gallery.toggle.on" label={t("gallery.sample.xo_peq")} checked onChange={() => {}} />
        <Toggle id="gallery.toggle.disabled" label={t("gallery.sample.mimo")} hint={t("gallery.sample.mimo.disabled")} checked={false} disabled onChange={() => {}} />
        <Segmented
          id="gallery.segmented.theme"
          label={t("frame.settings.theme")}
          value={theme}
          onChange={setTheme}
          options={[
            { value: "system", label: t("frame.settings.theme.system") },
            { value: "light", label: t("frame.settings.theme.light") },
            { value: "dark", label: t("frame.settings.theme.dark") },
          ]}
        />
      </div>
    </div>
  );
}

type TableRow = { role: string; out: number; pol: string; size: string };

function TableSpecimen({ empty = false }: { empty?: boolean }) {
  const t = useT();
  const [selected, setSelected] = useState<string | null>("C");
  const rows = (roles: string[], start: number): TableRow[] => roles.map((role, i) => ({ role, out: start + i, pol: i === 1 && role.startsWith("SW") ? t("gallery.sample.pol.inverted") : t("gallery.sample.pol.normal"), size: role.startsWith("SW") ? "—" : t("gallery.sample.size.small") }));
  const groups = empty
    ? [{ id: "none", label: "", rows: [] }]
    : [
        { id: "front", label: t("gallery.sample.group.front"), rows: rows(["L", "R"], 1) },
        { id: "center", label: t("gallery.sample.group.center"), rows: rows(["C"], 3) },
        { id: "sub", label: t("gallery.sample.group.sub"), rows: rows(["SW1", "SW2"], 4) },
        { id: "top", label: t("gallery.sample.group.top"), rows: rows(["Tfl", "Tfr"], 10) },
      ];
  return (
    <DataTable<TableRow>
      id={empty ? "gallery.table.empty" : "gallery.table"}
      caption={t("gallery.sample.table")}
      showCaption
      columns={[
        { key: "role", header: t("gallery.sample.col.channel"), cell: (r) => <strong>{roleCode(r.role)}</strong> },
        { key: "size", header: t("gallery.sample.col.size"), cell: (r) => r.size },
        { key: "out", header: t("gallery.sample.col.output"), numeric: true, width: "7rem", cell: (r) => r.out },
        { key: "pol", header: t("gallery.sample.col.polarity"), cell: (r) => r.pol },
      ]}
      groups={groups}
      rowKey={(r) => r.role}
      selectedKey={selected}
      onSelect={(r) => setSelected(r.role)}
      empty={<EmptyState compact title={t("gallery.sample.table.empty")} description={t("gallery.sample.table.empty_hint")} />}
    />
  );
}

function Meters() {
  const t = useT();
  return (
    <div className="gal-stack gal-narrow">
      <LevelMeter id="gallery.meter.none" label={roleCode("L")} value={null} />
      <LevelMeter id="gallery.meter.low" label={roleCode("R")} value={-41.5} peak={-33} />
      <LevelMeter id="gallery.meter.ok" label={roleCode("C")} value={-22.4} peak={-14.8} />
      <LevelMeter id="gallery.meter.high" label={roleCode("SW1")} value={-9.6} peak={-4.2} />
      <LevelMeter id="gallery.meter.clip" label={roleCode("Tfl")} value={-1.2} peak={0} clipping />
      <div className="gal-stack">
        <span className="muted">{t("jobs.kind.calibrate")}</span>
        <ProgressBar label={t("jobs.kind.calibrate")} done={5} total={11} valueText="5/11" />
        <span className="muted">{t("jobs.kind.sample_load")}</span>
        <ProgressBar label={t("jobs.kind.sample_load")} indeterminate />
      </div>
    </div>
  );
}

function Toasts() {
  const t = useT();
  return (
    <ol className="ui-toasts__list gal-narrow">
      <ToastView item={{ message: t("gallery.sample.toast.cleared", { n: 3 }), action: { label: t("ui.toast.undo"), onAction: () => {} }, duration: 0 }} onClose={() => {}} />
      <ToastView item={{ message: t("gallery.sample.toast.saved"), tone: "success", duration: 0 }} onClose={() => {}} />
      <ToastView item={{ message: t("jobs.log.measurement_low_snr", { code: "SW2", snr_db: "17.2", threshold_db: "20" }), tone: "warning", duration: 0 }} onClose={() => {}} />
      <ToastView item={{ message: `${t("error.export_failed")} ${t("error.export_failed.next")}`, tone: "danger", duration: 0 }} onClose={() => {}} />
    </ol>
  );
}

function Dialogs() {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="gal-grid-2">
      <ConfirmDialog
        inline
        open
        id="gallery.dialog.clear"
        onClose={() => {}}
        onConfirm={() => {}}
        title={t("gallery.sample.clear_title", { n: 3 })}
        body={t("gallery.sample.clear_body")}
        confirmLabel={t("gallery.sample.clear_position")}
      />
      <div className="gal-stack">
        <ConfirmUndoDemo />
        <Button onClick={() => setOpen(true)}>{t("gallery.sample.open_dialog")}</Button>
        <Dialog open={open} onClose={() => setOpen(false)} title={t("frame.settings")} footer={<Button variant="primary" onClick={() => setOpen(false)}>{t("ui.close")}</Button>}>
          <p>{t("frame.settings.expert.hint")}</p>
        </Dialog>
      </div>
    </div>
  );
}

/** U5: confirm, clear, then undo from the toast. */
function ConfirmUndoDemo() {
  const t = useT();
  const [positions, setPositions] = useState([1, 2, 3]);
  const last = positions[positions.length - 1];
  const { request, dialog } = useConfirmAndUndo({
    id: "gallery.clear",
    title: t("gallery.sample.clear_title", { n: last ?? 0 }),
    body: t("gallery.sample.clear_body"),
    confirmLabel: t("gallery.sample.clear_position"),
    done: t("gallery.sample.toast.cleared", { n: last ?? 0 }),
    run: async () => {
      setPositions((p) => p.slice(0, -1));
      return `undo-${last}`;
    },
    undo: async () => setPositions((p) => (last !== undefined && !p.includes(last) ? [...p, last].sort() : p)),
  });
  return (
    <div className="gal-row">
      <span className="tabular">{t("frame.status.positions.value", { done: positions.length, total: 9 })}</span>
      <Button variant="danger" size="sm" onClick={request} disabled={!last}>
        {t("gallery.sample.clear_position")}
      </Button>
      {dialog}
    </div>
  );
}

function TooltipSpecimen() {
  const t = useT();
  return (
    <div className="gal-tooltip-stage">
      <Tooltip content={t("gallery.sample.tooltip.long")} staticOpen>
        <Button variant="secondary">{t("gallery.sample.xo_peq")}</Button>
      </Tooltip>
      <Tooltip content={t("plot.axes.tooltip")}>
        <IconButton icon="axes" label={t("plot.axes.button")} />
      </Tooltip>
    </div>
  );
}

function States() {
  const t = useT();
  return (
    <div className="gal-grid-2">
      <Card dense>
        <EmptyState
          title={t("gallery.sample.empty.title")}
          description={t("gallery.sample.empty.body")}
          action={<Button variant="primary">{t("frame.step.measure.primary")}</Button>}
        />
      </Card>
      <div className="gal-stack">
        <ErrorNotice
          cause={t("error.rew_unreachable")}
          next={t("error.rew_unreachable.next")}
          code="rew_unreachable"
          detail="GET http://127.0.0.1:4735/version: connection refused (os error 10061)"
          action={<Button size="sm">{t("gallery.sample.retry")}</Button>}
        />
        <ErrorNotice tone="warning" cause={t("error.stale_inputs")} next={t("error.stale_inputs.next")} />
      </div>
    </div>
  );
}

export const kitSpecimens: SpecimenDef[] = [
  { id: "ui.button", title: "Buttons · primary, secondary, quiet, danger; busy, disabled, small, icon", render: () => <Buttons /> },
  { id: "ui.card", title: "Card · normal, stale, dense with sections and 고급", render: () => <Cards /> },
  { id: "ui.pill", title: "Status pill · every tone", render: () => <Pills /> },
  { id: "ui.number", title: "Number field · normal, with range, invalid, disabled", render: () => <NumberFields /> },
  { id: "ui.choice", title: "Select, stepper, toggle, segmented", render: () => <Choices /> },
  { id: "ui.table", title: "Table · group rows, numeric column, selected row", render: () => <TableSpecimen /> },
  { id: "ui.table.empty", title: "Table · empty", render: () => <TableSpecimen empty /> },
  { id: "ui.meter", title: "Level meter · none, low, in range, high, clipping; progress", render: () => <Meters /> },
  { id: "ui.toast", title: "Toasts · undo, success, warning, error", render: () => <Toasts /> },
  { id: "ui.dialog", title: "Dialog · confirm-and-undo (U5)", render: () => <Dialogs /> },
  { id: "ui.tooltip", title: "Tooltip · open (800 ms on hover/focus), width cap", render: () => <TooltipSpecimen /> },
  { id: "ui.states", title: "Empty and error states", render: () => <States /> },
];
