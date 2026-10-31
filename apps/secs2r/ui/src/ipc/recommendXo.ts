import type { RecommendXoParams } from "./bindings/RecommendXoParams";
import type { Request } from "./bindings/Request";

/** The service accepts params at its JSON boundary; Rust keeps the CLI's unit variant. */
export type RecommendXoRequest = Extract<Request, { method: "recommend_xo" }> & { params?: RecommendXoParams };
export function recommendXoRequest(background = false): RecommendXoRequest {
  return background ? { method: "recommend_xo", params: { background: true } } : { method: "recommend_xo" };
}
export function isBackgroundRequest(request: Request): boolean {
  if (request.method === "calibrate") return request.params.background === true;
  return request.method === "recommend_xo" && (request as RecommendXoRequest).params?.background === true;
}
