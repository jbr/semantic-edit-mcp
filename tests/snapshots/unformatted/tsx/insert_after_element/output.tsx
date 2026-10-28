export function Page({ title }: { title: string }) {
  return (
    <main>
      <h1>{title}</h1>
      <p className="byline">By the team.</p>
      <p className="intro">Welcome.</p>
    </main>
  );
}
