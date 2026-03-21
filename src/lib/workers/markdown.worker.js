import { marked } from 'marked';

const renderer = new marked.Renderer();
renderer.table = (token) => {
  const defaultHtml = marked.Renderer.prototype.table.call(renderer, token);
  return `<div class="table-wrapper">${defaultHtml}</div>`;
};

marked.use({ renderer });

self.onmessage = ({ data: { id, text } }) => {
  const html = marked.parse(text ?? '');
  self.postMessage({ id, html });
};
