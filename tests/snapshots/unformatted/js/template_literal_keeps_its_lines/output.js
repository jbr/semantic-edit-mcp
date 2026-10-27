function page(title) {
  if (title) {
    const html = `
<h1>${title}</h1>
  <p>Welcome.</p>
`;
    console.log(html);
    return html;
  }
  return null;
}
