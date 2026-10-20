// A file server for the static build, shared by the screenshot scripts
// (gallery-shots.mjs, screen-tour.mjs). The build uses root-absolute asset
// paths, so serving dist at the root is all the pages need.
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, normalize, resolve } from "node:path";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".json": "application/json",
};

/** The HTTP server and optional browser route use exactly the same response. */
async function staticResponse(root, index, url, signal) {
  const empty = (status) => ({ status, headers: {}, body: Buffer.alloc(0) });
  try {
    const path = normalize(decodeURIComponent(new URL(url, "http://x").pathname));
    // Chromium asks for a favicon the app does not have.
    if (path === normalize("/favicon.ico")) return empty(204);
    const file = join(root, path === normalize("/") ? index : path);
    if (!file.startsWith(root)) return empty(404);
    // Finish the read before committing a response, so a read failure cannot
    // leave a successful status with a truncated body.
    const body = await readFile(file, { signal });
    return {
      status: 200,
      headers: {
        "content-type": TYPES[extname(file)] ?? "application/octet-stream",
        "content-length": String(body.length),
      },
      body,
    };
  } catch (error) {
    if (signal?.aborted) throw error;
    const missing = ["ENOENT", "ENOTDIR", "EISDIR"].includes(error?.code);
    const status = error instanceof URIError ? 400 : missing ? 404 : 500;
    if (status === 500) console.error(`static server: ${url}: ${error?.message ?? error}`);
    return empty(status);
  }
}

/** Serves `root` on a free loopback port; "/" answers with `index`. */
export async function serveStatic(root, index = "gallery.html") {
  root = resolve(root);
  const server = createServer(async (request, response) => {
    const controller = new AbortController();
    const abort = () => controller.abort();
    request.once("aborted", abort);
    response.once("close", abort);
    response.once("error", abort);
    try {
      const result = await staticResponse(root, index, request.url ?? "/", controller.signal);
      if (!controller.signal.aborted && !response.destroyed) {
        response.writeHead(result.status, result.headers).end(result.body);
      }
    } catch (error) {
      if (!controller.signal.aborted && !response.destroyed) response.destroy(error);
    }
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  const address = server.address();
  const origin = `http://127.0.0.1:${typeof address === "object" && address ? address.port : 0}`;
  return {
    origin,
    // Fulfil only this server's origin from disk. Local HTTP filters can alter
    // HTML in transit; routed captures must inspect the actual build bytes.
    route: (context) => context.route((url) => url.origin === origin, async (route) => {
      const page = route.request().frame().page();
      const controller = new AbortController();
      const abort = () => controller.abort();
      page.once("close", abort);
      context.once("close", abort);
      try {
        const result = await staticResponse(root, index, route.request().url(), controller.signal);
        if (!controller.signal.aborted && !page.isClosed()) await route.fulfill(result);
      } catch (error) {
        if (!controller.signal.aborted && !page.isClosed()) throw error;
      } finally {
        page.off("close", abort);
        context.off("close", abort);
      }
    }),
    close: () => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())),
  };
}
