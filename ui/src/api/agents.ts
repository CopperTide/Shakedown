import type { CheckAnswer, CheckBody } from '@/model/connection';
import { post } from './client';

/** The Connect screen's handshake: arenad says hello to an HTTP agent, or makes a 1-token Bedrock call. */
export const checkAgent = (body: CheckBody) => post<CheckAnswer>('/api/agents/check', body);
