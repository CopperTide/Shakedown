/** `n` and the word, plural when n is not 1: `1 run`, `2 runs`, `3 people` (`many` given). */
export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
