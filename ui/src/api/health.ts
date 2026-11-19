import type { Health, ProviderStatus } from '@/model/types';
import { get } from './client';

export const health = () => get<Health>('/api/health');
/** Every model provider; asks local servers (and OpenAI/Anthropic when keyed) for their models, 3 s each. */
export const providers = () => get<ProviderStatus[]>('/api/providers');
