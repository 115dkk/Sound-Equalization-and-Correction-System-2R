// The local API (ADR 0016) in the settings popover and its dialog, one
// specimen per state: off, listening, port taken, start failed, token
// shown, the confirm before a new token, and a port that cannot be applied.
// The token is a sample of the real length (hex digits in order), never a
// real one.
import { LocalApiDialog, type LocalApiDialogInitial } from "../app/LocalApiDialog";
import type { LocalApi, LocalApiView } from "../app/localApi";
import { SettingsButton } from "../app/menus";
import type { LocalApiStatus } from "../ipc/bindings/LocalApiStatus";
import type { SpecimenDef } from "./Specimen";

const SAMPLE_TOKEN = "0123456789abcdef".repeat(4);
const SAMPLE_PATH = "C:\\Users\\sample\\AppData\\Roaming\\io.github.115dkk.secs2r\\local-api-token";

const status = (patch: Partial<LocalApiStatus> = {}): LocalApiStatus => ({
  enabled: true,
  port: 4712,
  running: true,
  error: null,
  url: "http://127.0.0.1:4712",
  token: SAMPLE_TOKEN,
  token_path: SAMPLE_PATH,
  ...patch,
});

const api = (view: Partial<LocalApiView>): LocalApi => ({
  view: { status: status(), pending: null, error: null, ...view },
  actions: { setEnabled: () => {}, setPort: () => {}, regenerateToken: () => {}, copy: () => {} },
});

const OFF = api({ status: status({ enabled: false, running: false }) });
const LISTENING = api({});
const PORT_BUSY = api({ status: status({ running: false, error: "local API port 4712 is busy" }) });
const FAILED = api({ status: status({ running: false, error: "local API listener on port 4712: An attempt was made to access a socket in a way forbidden by its access permissions. (os error 10013)" }) });

function Popover({ localApi }: { localApi: LocalApi }) {
  return (
    <div className="gal-rail-sample gal-rail-sample--popover">
      <SettingsButton initiallyOpen inline localApi={localApi} />
    </div>
  );
}

function Dialog({ localApi, initial }: { localApi: LocalApi; initial?: LocalApiDialogInitial }) {
  return <LocalApiDialog inline api={localApi} onClose={() => {}} {...(initial ? { initial } : {})} />;
}

function Pair({ localApi }: { localApi: LocalApi }) {
  return (
    <div className="gal-pair">
      <Popover localApi={localApi} />
      <Dialog localApi={localApi} />
    </div>
  );
}

export const localApiSpecimens: SpecimenDef[] = [
  { id: "frame.local_api.off", title: "Local API · off (settings popover)", width: 380, render: () => <Popover localApi={OFF} /> },
  { id: "frame.local_api.listening", title: "Local API · on and listening (popover and dialog)", width: 1000, render: () => <Pair localApi={LISTENING} /> },
  { id: "frame.local_api.port_busy", title: "Local API · the port is taken (popover and dialog)", width: 1000, render: () => <Pair localApi={PORT_BUSY} /> },
  { id: "frame.local_api.failed", title: "Local API · did not start for another reason (raw text behind 자세히)", width: 1000, render: () => <Pair localApi={FAILED} /> },
  { id: "frame.local_api.token_shown", title: "Local API dialog · token shown", width: 640, render: () => <Dialog localApi={LISTENING} initial={{ tokenShown: true }} /> },
  { id: "frame.local_api.token_confirm", title: "Local API dialog · the one-line confirm before a new token", width: 640, render: () => <Dialog localApi={LISTENING} initial={{ confirming: true }} /> },
  { id: "frame.local_api.invalid_port", title: "Local API dialog · a port that cannot be applied", width: 640, render: () => <Dialog localApi={LISTENING} initial={{ portDraft: "80" }} /> },
];
