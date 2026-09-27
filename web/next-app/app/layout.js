// Next.js (app router) leg of the pnpm+nodejs+nextjs benchmark stack.
// Renders the same content as the Aoxn and Node servers.

export const metadata = {
  title: "Aoxn Web Benchmark",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
