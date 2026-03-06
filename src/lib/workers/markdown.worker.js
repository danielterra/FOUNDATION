import { marked } from 'marked';

self.onmessage = ({ data: { id, text } }) => {
  const html = marked.parse(text ?? '');
  self.postMessage({ id, html });
};
