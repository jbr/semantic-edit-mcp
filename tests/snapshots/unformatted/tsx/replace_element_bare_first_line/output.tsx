export function Page({ title }: { title: string }) {
  return (
    <main>
      <h1>{title}</h1>
      <section className="intro">
        <p>Welcome.</p>
        <p>Glad you're here.</p>
      </section>
    </main>
  );
}
