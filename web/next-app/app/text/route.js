// Route handler — same body as the Aoxn and Node servers' /text.

export function GET() {
  return new Response("hello, web\n", {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}
