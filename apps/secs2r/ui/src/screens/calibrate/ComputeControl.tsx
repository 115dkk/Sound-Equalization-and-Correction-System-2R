import type { MessageKey } from "../../i18n";
import { useT } from "../../i18n/react";
import type { ComputeReason } from "../../ipc/bindings/ComputeReason";
import type { ComputeReport } from "../../ipc/bindings/ComputeReport";
import { Button, ErrorNotice, Select } from "../../ui";
import type { CalibrateActions, CalibrateView } from "./types";

const REASONS: Record<ComputeReason, MessageKey> = {
  recommended: "compute.reason.recommended", user_cpu: "compute.reason.user_cpu", user_gpu: "compute.reason.user_gpu",
  no_device: "compute.reason.no_device", runtime_unavailable: "compute.reason.runtime_unavailable",
  unsupported_platform: "compute.reason.unsupported_platform", integrated: "compute.reason.integrated",
  legacy: "compute.reason.legacy", low_performance: "compute.reason.low_performance",
  insufficient_memory: "compute.reason.insufficient_memory", device_failure: "compute.reason.device_failure",
  small_problem: "compute.reason.small_problem", iir_reference: "compute.reason.iir_reference",
};

/** Availability describes the next run; provenance below describes the saved result. */
export function ComputeControl({ view, actions }: { view: CalibrateView; actions: CalibrateActions }) {
  const t = useT();
  const { preference, status, loading, saving, error, locked } = view.compute;
  const requestedGpuUnavailable = preference === "gpu" && status?.gpu_available === false;
  const iirReference = !view.settings.mimo_fir;
  const busy = locked || view.running !== null || view.measuring;
  const disabled = busy || loading || saving || error !== null || status === null;
  return (
    <div className="calibrate-compute" data-ui="calibrate.compute" aria-busy={loading || saving}>
      <Select id="calibrate.compute.preference" label={t("compute.label")} value={preference}
        options={[{ value: "auto", label: t("compute.auto") }, { value: "cpu", label: "CPU" },
          { value: "gpu", label: "GPU", disabled: !status?.gpu_available }]}
        disabled={disabled} onChange={actions.changeCompute} help={t("compute.hint")} />
      <div className="calibrate-compute__state" role="status" aria-live="polite">
        {loading ? <p>{t("compute.loading")}</p> : saving ? <p>{t("compute.saving")}</p> : !error && status ? <>
          <p className="calibrate-compute__device">{requestedGpuUnavailable ? t("compute.selected.unavailable") : !iirReference && status.selected === "cuda"
            ? t("compute.selected.gpu", { device: status.device?.name ?? "GPU" }) : t("compute.selected.cpu")}</p>
          <p className="calibrate-hint">{t(requestedGpuUnavailable ? "compute.selected.unavailable.next" : iirReference ? "compute.reason.iir_reference" : REASONS[status.reason])}</p>
        </> : null}
        {busy && <p className="calibrate-hint">{t("compute.locked")}</p>}
      </div>
      {error && <ErrorNotice id="calibrate.compute.error" cause={t("compute.error")} next={t("compute.error.next")}
        detail={error.detail} code={error.code}
        action={<Button disabled={busy || loading} onClick={actions.refreshCompute}>{t("compute.retry")}</Button>} />}
      {!loading && !error && status?.reason === "runtime_unavailable" && <details className="ui-details calibrate-compute__help">
        <summary>{t("compute.install.title")}</summary>
        <p>{t("compute.install.description")}</p>
        <Button variant="quiet" onClick={actions.openGpuHelp}>{t("compute.install.open")}</Button>
      </details>}
    </div>
  );
}

export function ComputeResult({ report }: { report: ComputeReport }) {
  const t = useT();
  return <div className="calibrate-compute-result" data-ui="calibrate.compute.result">
    <p>{report.backend === "cuda" && report.stage === "ideal_fir_sweep"
      ? t("compute.result.gpu", { device: report.device ?? "GPU" }) : t("compute.result.cpu")}</p>
    {report.fallbacks > 0 ? <p className="calibrate-hint">{t("compute.result.fallback", { count: report.fallbacks })}</p>
      : report.reason === "iir_reference" ? <p className="calibrate-hint">{t("compute.result.iir_reference")}</p>
      : report.reason === "small_problem" ? <p className="calibrate-hint">{t("compute.reason.small_problem")}</p> : null}
  </div>;
}
