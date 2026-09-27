// Route handler — same JSON as the Aoxn and Node servers' /api/json.

export function GET() {
  const squares = [];
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    squares.push(sq);
    total += sq;
  }
  return Response.json({
    language: "Aoxn",
    version: "0.26.3",
    squares,
    sum: total,
  });
}
