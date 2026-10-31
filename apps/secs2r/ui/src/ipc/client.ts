// The screen's only door to the Rust side (docs/adr/0003). Every call goes
// through the `secs_call` command and comes back as an envelope; the command
// never rejects, so a rejection here means the bridge itself is broken.
import type { IpcError } from "./bindings/IpcError";
import type { Request } from "./bindings/Request";
import type { Response } from "./bindings/Response";

/** The reply shape pinned by `secs-types`' `envelope_wire_shape` test. */
export type Envelope = { ok: true; data: Response } | { ok: false; error: IpcError };

/** The response type that belongs to request method `M`. */
export type ResponseOf<M extends Request["method"]> = Extract<Response, { method: M }>["data"];

export type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;

export class IpcFailure extends Error {
  readonly error: IpcError;
  constructor(error: IpcError) {
    super(error.detail || error.code);
    this.name = "IpcFailure";
    this.error = error;
  }
}

/** Builds a typed caller over an invoke function (Tauri's, or a fake in tests). */
export function makeClient(call: Invoke) {
  return async function request<M extends Request["method"]>(
    req: Extract<Request, { method: M }>,
  ): Promise<ResponseOf<M>> {
    const envelope = (await call("secs_call", { request: req })) as Envelope;
    if (!envelope.ok) {
      throw new IpcFailure(envelope.error);
    }
    if (envelope.data.method !== req.method) {
      throw new IpcFailure({
        code: "internal",
        params: {},
        retryable: false,
        detail: `reply for ${envelope.data.method} to a ${req.method} request`,
      });
    }
    return envelope.data.data as ResponseOf<M>;
  };
}
