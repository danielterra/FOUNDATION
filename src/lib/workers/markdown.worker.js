import { parseMarkdown } from '../components/widgets/inspector/markdownPrivacy.js';

self.onmessage = ({ data: { id, text } }) => {
  try {
    let html = parseMarkdown(text ?? '');
    html = html
      .replace(/<table>/g, '<div class="table-wrapper"><table>')
      .replace(/<\/table>/g, '</table></div>');
    self.postMessage({ id, html });
  } catch {
    self.postMessage({ id, html: text ?? '' });
  }
};
