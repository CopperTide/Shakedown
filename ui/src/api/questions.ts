// `/api/branches/{id}/questions`: questions asked about a branch's steps and their checked
// answers (the `questions.list` and `questions.ask` operations).
import type { StepQuestion, StoredReview } from '@/model/review';
import { get, path, post, qs } from './client';

export type StoredQuestion = StoredReview<StepQuestion>;

/** The questions asked about step `seq` of `branch`, oldest first. */
export const stepQuestions = async (branch: string, seq: number) =>
  (await get<{ questions: StoredQuestion[] }>(path`/api/branches/${branch}/questions` + qs({ seq: String(seq) }))).questions;

/** Asks a question about step `seq`; answers at once with it `running`. A blank `model` is the review model. */
export const askStep = (branch: string, seq: number, question: string, model?: string) =>
  post<StoredQuestion>(path`/api/branches/${branch}/questions`, { seq, question, ...(model?.trim() ? { model: model.trim() } : {}) });
