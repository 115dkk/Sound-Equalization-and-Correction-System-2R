// A file server for the static build, shared by the screenshot scripts
// (gallery-shots.mjs, screen-tour.mjs). The build uses root-absolute asset
// paths, so serving dist at the root is all the pages need.
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".json": "application/json",
};

/** Serves `root` on a free loopback port; "/" answers with `index`. */
export async function serveStatic(root, index = "gallery.html") {
  const server = createServer((request, response) => {
    const path = normalize(decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname));
    // Chromium asks for a favicon the app does not have.
    if (path === normalize("/favicon.ico")) {
      response.writeHead(204).end();
      return;
    }
    const file = join(root, path === "/" ? index : path);
    if (!file.startsWith(root) || !existsSync(file) || !statSync(file).isFile()) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    createReadStream(file).pipe(response);
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  const address = server.address();
  const origin = `http://127.0.0.1:${typeof address === "object" && address ? address.port : 0}`;
  return { origin, close: () => server.close() };
}
