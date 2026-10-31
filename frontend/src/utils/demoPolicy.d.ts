import type { ResidencyPolicy } from '../types';
export function policyForSelection(value: string): ResidencyPolicy;
export function validateDemoPolicy(policy: unknown): asserts policy is ResidencyPolicy;
