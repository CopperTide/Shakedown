import { describe, expect, it } from 'vitest';
import { MARK, alarmMark, passMark } from './mark';

describe('marks', () => {
  it('pass or fail, with the words given', () => {
    expect(passMark(true)).toEqual({ glyph: '✓', word: 'Passed', abnormal: false });
    expect(passMark(false)).toEqual({ glyph: '✕', word: 'Failed', abnormal: true });
    expect(passMark(false, ['Pass', 'Fail'])).toEqual({ glyph: '✕', word: 'Fail', abnormal: true });
    expect(alarmMark('refused')).toEqual({ glyph: '⚠', word: 'refused', abnormal: true });
    expect(MARK.finished).toEqual({ glyph: '■', word: 'Finished', abnormal: false });
  });
});
