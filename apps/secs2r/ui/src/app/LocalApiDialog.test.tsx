// @vitest-environment happy-dom
// The local API in the settings popover (switch, state line, the button to
// the dialog) and the 로컬 API dialog (masked token, confirm before a new
// token, port field).
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi, type Mock } from "vitest";
import type { LocalApiStatus } from "../ipc/bindings/LocalApiStatus";
import { LocalApiDialog, type LocalApiDialogInitial } from "./LocalApiDialog";
import type { LocalApi, LocalApiActions, LocalApiView } from "./localApi";
import { SettingsButton } from "./menus";
import { PrefsScope } from "./prefs";

afterEach(cleanup);

const TOKEN = "0123456789abcdef".repeat(4);

const status = (patch: Partial<LocalApiStatus> = {}): LocalApiStatus => ({
  enabled: true,
  port: 4712,
  running: true,
  error: null,
  url: "http://127.0.0.1:4712",
  token: TOKEN,
  token_path: "C:\\sample\\local-api-token",
  ...patch,
});

function makeApi(view: Partial<LocalApiView> = {}): LocalApi & { actions: { [K in keyof LocalApiActions]: Mock<LocalApiActions[K]> } } {
  return {
    view: { status: status(), pending: null, error: null, ...view },
    actions: {
      setEnabled: vi.fn<LocalApiActions["setEnabled"]>(),
      setPort: vi.fn<LocalApiActions["setPort"]>(),
      regenerateToken: vi.fn<LocalApiActions["regenerateToken"]>(),
      copy: vi.fn<LocalApiActions["copy"]>(),
    },
  };
}

function popover(api: LocalApi, inline = true) {
  return render(
    <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
      <SettingsButton initiallyOpen inline={inline} localApi={api} />
    </PrefsScope>,
  );
}

function dialog(api: LocalApi, initial?: LocalApiDialogInitial) {
  const onClose = vi.fn();
  render(
    <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
      <LocalApiDialog inline api={api} onClose={onClose} {...(initial ? { initial } : {})} />
    </PrefsScope>,
  );
  return { onClose, box: screen.getByRole("dialog", { name: "로컬 API" }) };
}

const ui = (id: string) => {
  const node = document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};

describe("settings popover row", () => {
  it("shows off, and turns the local API on from the switch without the dialog", () => {
    const api = makeApi({ status: status({ enabled: false, running: false }) });
    popover(api);
    const toggle = screen.getByRole("switch", { name: "로컬 API" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(ui("frame.settings.local_api.row").textContent).toContain("꺼짐");
    fireEvent.click(toggle);
    expect(api.actions.setEnabled).toHaveBeenCalledWith(true);
    expect(screen.queryByRole("dialog", { name: "로컬 API" })).toBeNull();
  });

  it("shows where it listens, and the state is the switch's description", () => {
    popover(makeApi());
    const toggle = screen.getByRole("switch", { name: "로컬 API" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    const described = document.getElementById(toggle.getAttribute("aria-describedby") ?? "");
    expect(described?.textContent).toBe("127.0.0.1:4712에서 받는 중");
    expect(screen.getByRole("button", { name: "연결 정보" })).toBeTruthy();
  });

  it("says the port is taken and offers to change it", () => {
    popover(makeApi({ status: status({ running: false, error: "local API port 4712 is busy" }) }));
    expect(ui("frame.settings.local_api.row").textContent).toContain("4712번 포트를 다른 프로그램이 쓰고 있습니다.");
    expect(screen.getByRole("button", { name: "포트 바꾸기…" })).toBeTruthy();
  });

  it("says which way the switch is going while it saves, and drops a second press", () => {
    const api = makeApi({ status: status({ enabled: false, running: false }), pending: "toggle" });
    popover(api);
    expect(ui("frame.settings.local_api.row").textContent).toContain("켜는 중");
    fireEvent.click(screen.getByRole("switch", { name: "로컬 API" }));
    expect(api.actions.setEnabled).not.toHaveBeenCalled();
  });

  it("keeps the switch off until the first status arrives", () => {
    popover(makeApi({ status: null }));
    expect((screen.getByRole("switch", { name: "로컬 API" }) as HTMLButtonElement).disabled).toBe(true);
    expect(ui("frame.settings.local_api.row").textContent).toContain("확인 중");
  });

  it("replaces the popover with the dialog, on the port field when the port is taken", async () => {
    popover(makeApi({ status: status({ running: false, error: "local API port 4712 is busy" }) }), false);
    fireEvent.click(screen.getByRole("button", { name: "포트 바꾸기…" }));
    await act(async () => {});
    expect(screen.queryByRole("switch", { name: "로컬 API" })).toBeNull();
    const box = screen.getByRole("dialog", { name: "로컬 API" });
    expect(document.activeElement).toBe(within(box).getByLabelText("포트"));
    expect(box.textContent).toContain("다른 포트를 넣고 포트 바꾸기를 누르세요.");
  });
});

describe("token", () => {
  it("stays masked until 보기, and copies the real token either way", () => {
    const api = makeApi();
    const { box } = dialog(api);
    expect(box.textContent).not.toContain(TOKEN);
    expect(ui("frame.local_api.token.value").textContent).toContain("숨긴 토큰");

    fireEvent.click(within(box).getByRole("button", { name: "토큰 복사" }));
    expect(api.actions.copy).toHaveBeenCalledWith(TOKEN);

    fireEvent.click(within(box).getByRole("button", { name: "토큰 보기" }));
    expect(ui("frame.local_api.token.value").textContent).toBe(TOKEN);
    fireEvent.click(within(box).getByRole("button", { name: "토큰 숨기기" }));
    expect(box.textContent).not.toContain(TOKEN);
  });

  it("asks in one line before making a new token, and 취소 or Escape leaves it", () => {
    const api = makeApi();
    const { box, onClose } = dialog(api);
    const confirmLine = "지금 토큰을 쓰는 프로그램에는 새 토큰을 다시 넣어야 합니다.";

    fireEvent.click(within(box).getByRole("button", { name: "새 토큰 만들기" }));
    expect(api.actions.regenerateToken).not.toHaveBeenCalled();
    const group = within(box).getByRole("group", { name: confirmLine });
    fireEvent.click(within(group).getByRole("button", { name: "취소" }));
    expect(box.textContent).not.toContain(confirmLine);
    expect(api.actions.regenerateToken).not.toHaveBeenCalled();

    fireEvent.click(within(box).getByRole("button", { name: "새 토큰 만들기" }));
    fireEvent.keyDown(within(box).getByRole("button", { name: "취소" }), { key: "Escape" });
    expect(box.textContent).not.toContain(confirmLine);
    expect(onClose).not.toHaveBeenCalled();
    expect(api.actions.regenerateToken).not.toHaveBeenCalled();

    fireEvent.click(within(box).getByRole("button", { name: "새 토큰 만들기" }));
    fireEvent.click(within(within(box).getByRole("group", { name: confirmLine })).getByRole("button", { name: "새 토큰 만들기" }));
    expect(api.actions.regenerateToken).toHaveBeenCalledTimes(1);
    expect(box.textContent).not.toContain(confirmLine);
  });
});

describe("port", () => {
  it("applies only a changed, valid port, and says why it cannot apply under the field", () => {
    const api = makeApi();
    const { box } = dialog(api);
    const input = within(box).getByLabelText("포트") as HTMLInputElement;
    const apply = within(box).getByRole("button", { name: "포트 바꾸기" }) as HTMLButtonElement;
    expect(input.value).toBe("4712");
    expect(apply.disabled).toBe(true);

    fireEvent.change(input, { target: { value: "80" } });
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(document.getElementById(input.getAttribute("aria-describedby") ?? "")?.textContent).toBe("1024–65535 사이의 포트를 넣으세요.");
    expect(apply.disabled).toBe(true);

    fireEvent.change(input, { target: { value: "47a" } });
    expect(box.textContent).toContain("숫자만 넣으세요. 범위는 1024–65535입니다.");

    fireEvent.change(input, { target: { value: "5000" } });
    expect(input.getAttribute("aria-invalid")).toBeNull();
    expect(apply.disabled).toBe(false);
    fireEvent.click(apply);
    expect(api.actions.setPort).toHaveBeenCalledWith(5000);

    fireEvent.change(input, { target: { value: "5001" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(api.actions.setPort).toHaveBeenLastCalledWith(5001);
  });

  it("puts the saved port back on the first Escape", () => {
    const { box, onClose } = dialog(makeApi(), { portDraft: "80" });
    const input = within(box).getByLabelText("포트") as HTMLInputElement;
    expect(input.value).toBe("80");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("4712");
    expect(onClose).not.toHaveBeenCalled();
  });
});
