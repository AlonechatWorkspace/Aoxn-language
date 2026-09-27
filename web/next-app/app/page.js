// SSR page — same content as web/http_buf.ax body_html() and the Node server.

export default function Page() {
  const rows = [];
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    total += sq;
    rows.push(
      <tr key={i}>
        <td>{i}</td>
        <td>{sq}</td>
      </tr>
    );
  }
  return (
    <main>
      <h1>Aoxn Web Benchmark</h1>
      <p>Server-side rendered for the Aoxn web benchmark.</p>
      <table>
        <thead>
          <tr>
            <th>n</th>
            <th>n*n</th>
          </tr>
        </thead>
        <tbody>{rows}</tbody>
      </table>
      <p>Sum of squares 1..20: {total}</p>
    </main>
  );
}
