// Plain Node.js (node:http) server — the "nodejs" leg of the
// pnpm+nodejs+nextjs benchmark stack. Renders byte-identical bodies to the
// Aoxn server (web/http_buf.ax); add the same routes: /, /api/json, /text.

import { createServer } from "node:http";

function bodyHtml() {
  let rows = "";
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    total += sq;
    rows += `<tr><td>${i}</td><td>${sq}</td></tr>\n`;
  }
  return (
    "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>Aoxn Web Benchmark</title>\n</head>\n<body>\n" +
    "<h1>Aoxn Web Benchmark</h1>\n<p>Server-side rendered for the Aoxn web benchmark.</p>\n" +
    "<table>\n<tr><th>n</th><th>n*n</th></tr>\n" +
    rows +
    "</table>\n<p>Sum of squares 1..20: " + total + "</p>\n</body>\n</html>\n"
  );
}

function bodyJson() {
  const squares = [];
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    squares.push(sq);
    total += sq;
  }
  return JSON.stringify({ language: "Aoxn", version: "0.26.3", squares, sum: total });
}

const server = createServer((req, res) => {
  if (req.url === "/") {
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end(bodyHtml());
  } else if (req.url === "/api/json") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(bodyJson());
  } else if (req.url === "/text") {
    res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
    res.end("hello, web\n");
  } else {
    res.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" });
    res.end("not found\n");
  }
});

const port = Number(process.env.PORT ?? 3000);
server.listen(port, "127.0.0.1", () => {
  console.log("Node.js web server listening on port " + port);
});
