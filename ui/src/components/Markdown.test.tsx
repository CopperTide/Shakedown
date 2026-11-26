// @vitest-environment jsdom
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import Markdown from './Markdown';

describe('Markdown', () => {
  it('renders bold, lists and code instead of their marks', () => {
    const { container } = render(<Markdown text={'**UA525 (AUS):** delay\n\n1. Report findings\n2. Rebook `UA2206`'} />);
    expect(screen.getByText('UA525 (AUS):').tagName).toBe('STRONG');
    expect(container.querySelectorAll('ol li')).toHaveLength(2);
    expect(screen.getByText('UA2206').tagName).toBe('CODE');
    expect(container.textContent).not.toContain('**');
  });
  it('never renders raw HTML from the text', () => {
    const { container } = render(<Markdown text={'hi <img src=x onerror="alert(1)"> <script>bad()</script>'} />);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('script')).toBeNull();
  });
});
