import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { ApproximateMark } from './marks';

describe('ApproximateMark', () => {
  it('approximate branch is labelled', () => {
    const html = renderToStaticMarkup(<ApproximateMark of={{ restored: [{ part: 'retail', how: 'save' }, { part: 'plant', how: 'approximate' }] }} />);
    expect(html).toContain('>Approximate restore</span>');
    expect(html).toContain('title="retail: from its save\nplant: approximate"');
    expect(renderToStaticMarkup(<ApproximateMark of={{ restored: [{ part: 'plant', how: 'replay' }] }} />)).toBe('');
    expect(renderToStaticMarkup(<ApproximateMark of={{}} />)).toBe('');
  });
});
