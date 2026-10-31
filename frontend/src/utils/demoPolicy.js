const REGIONS = ['EU_WEST', 'US_EAST', 'AP_SOUTH'];

export function policyForSelection(value) {
  switch (value) {
    case 'EU_GDPR_STRICT': return { type: 'StrictLocal', description: 'Local-only demo policy' };
    case 'US_DATA_RESIDENCY': return { type: 'RestrictedReplication', allowedRegions: ['US_EAST'], description: 'US destination demo policy' };
    case 'GLOBAL_REPLICATED': return { type: 'GlobalCleartext', description: 'All configured demo regions' };
    default: throw new Error('Unknown demo policy selection');
  }
}

export function validateDemoPolicy(policy) {
  if (!policy || typeof policy !== 'object') throw new Error('A structured demo policy is required');
  if (!['StrictLocal', 'RestrictedReplication', 'GlobalCleartext', 'SealedCiphertext'].includes(policy.type)) {
    throw new Error('Unknown demo policy type');
  }
  if (['RestrictedReplication', 'SealedCiphertext'].includes(policy.type)
    && (!Array.isArray(policy.allowedRegions) || !policy.allowedRegions.every(region => REGIONS.includes(region)))) {
    throw new Error('Demo policy requires an explicit valid region list');
  }
  if (policy.type === 'SealedCiphertext') throw new Error('Sealed replication is not implemented; demo request rejected');
}
