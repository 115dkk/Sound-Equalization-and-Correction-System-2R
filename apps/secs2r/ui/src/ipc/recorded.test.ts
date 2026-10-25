import { describe, expect, it } from "vitest";
import type { Request } from "./bindings/Request";
import { createMockService } from "./mock";
import { RECORDED } from "./recorded";

/** The project crates/secs-service/tests/recorded.rs answers on: freshly created, 2.0. */
const created: Request = { method: "project_new", params: { path: "/recorded.secs", preset: "2.0", sub_count: 0, sample: false, tutorial: false } };

const cases = Object.entries(RECORDED).flatMap(([method, entries]) =>
  entries.map((entry) => [`${method} ${JSON.stringify(entry.params)}`, method, entry.params, entry.data] as const),
);

describe("recorded service responses", () => {
  it.each(cases)("the mock answers %s as recorded", async (_name, method, params, data) => {
    const mock = createMockService({ tickMs: 0, project: null });
    try {
      await mock.request(created);
      const request = (params === null ? { method } : { method, params }) as Request;
      expect(await mock.invoke("secs_call", { request })).toEqual({ ok: true, data: { method, data } });
    } finally { mock.dispose(); }
  });
});
