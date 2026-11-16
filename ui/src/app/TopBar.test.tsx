import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import TopBar from './TopBar';

const draw = () => renderToStaticMarkup(<MemoryRouter><TopBar onPalette={() => {}} /></MemoryRouter>);

describe('TopBar wordmark', () => {
  it('credits CopperTide in muted text right after "Shakedown"', () => {
    const html = draw();
    expect(html).toMatch(/Shakedown<\/span><span class="[^"]*text-muted[^"]*"[^>]*>by CopperTide<\/span>/);
  });
});
