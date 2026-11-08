import type { Request } from "./bindings/Request";

export type RecommendXoRequest = Extract<Request, { method: "recommend_xo" }>;
export function recommendXoRequest(background = false): RecommendXoRequest {
  return background ? { method: "recommend_xo", params: { background: true } } : { method: "recommend_xo" };
}
export function isBackgroundRequest(request: Request): boolean {
  if (request.method === "calibrate") return request.params.background === true;
  return request.method === "recommend_xo" && request.params?.background === true;
}
