import { describe, expect, it } from 'vitest'

import type { EntitlementsSnapshot } from './backend'
import { entitlementCapability, entitlementDisabledReason, isFeatureEntitled } from './entitlements'

const developerEntitlements: EntitlementsSnapshot = {
  tier: 'developer',
  source: 'env-override',
  capabilities: [
    {
      featureId: 'local-recording',
      state: 'enabled'
    },
    {
      featureId: 'livestreaming',
      state: 'developer-override',
      reason: 'Enabled by VIDEORC_PREMIUM_FEATURES=1.'
    },
    {
      featureId: 'cloud-ai',
      state: 'developer-override',
      reason: 'Enabled by VIDEORC_PREMIUM_FEATURES=1.'
    }
  ]
}

describe('entitlements', () => {
  it('keeps local recording enabled when the backend snapshot has not loaded yet', () => {
    expect(isFeatureEntitled(null, 'local-recording')).toBe(true)
    expect(entitlementDisabledReason(null, 'local-recording')).toBeNull()
  })

  it('treats livestreaming and cloud AI as disabled by default', () => {
    expect(isFeatureEntitled(null, 'livestreaming')).toBe(false)
    expect(entitlementDisabledReason(null, 'livestreaming')).toContain('Premium')
    expect(isFeatureEntitled(null, 'cloud-ai')).toBe(false)
    expect(entitlementDisabledReason(null, 'cloud-ai')).toContain('Premium')
  })

  it('treats developer override state as entitled', () => {
    expect(isFeatureEntitled(developerEntitlements, 'livestreaming')).toBe(true)
    expect(isFeatureEntitled(developerEntitlements, 'cloud-ai')).toBe(true)
    expect(entitlementDisabledReason(developerEntitlements, 'cloud-ai')).toBeNull()
  })

  it('returns a disabled fallback for a missing capability', () => {
    const snapshot: EntitlementsSnapshot = {
      tier: 'free',
      source: 'local-default',
      capabilities: []
    }

    expect(entitlementCapability(snapshot, 'livestreaming')).toMatchObject({
      featureId: 'livestreaming',
      state: 'disabled'
    })
  })
})
