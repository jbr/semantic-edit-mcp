export function Page({ title }: { title: string }) {
  return (
    <main>
      <h1>{title}</h1>
      <p className="intro">Welcome.</p>
    </main>
  );
}
