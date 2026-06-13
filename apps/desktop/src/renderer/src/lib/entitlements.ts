import type { EntitlementCapability, EntitlementsSnapshot, FeatureId } from './backend'

export const DEFAULT_FREE_ENTITLEMENTS: EntitlementsSnapshot = {
  tier: 'free',
  source: 'local-default',
  capabilities: [
    {
      featureId: 'local-recording',
      state: 'enabled'
    },
    {
      featureId: 'livestreaming',
      state: 'disabled',
      reason: 'Livestreaming is a Videorc Premium feature.'
    },
    {
      featureId: 'cloud-ai',
      state: 'disabled',
      reason: 'Cloud AI is a Videorc Premium feature.'
    }
  ]
}

export function entitlementCapability(
  snapshot: EntitlementsSnapshot | null,
  featureId: FeatureId
): EntitlementCapability {
  const capability = snapshot?.capabilities.find((item) => item.featureId === featureId)
  if (capability) {
    return capability
  }

  const fallback = DEFAULT_FREE_ENTITLEMENTS.capabilities.find(
    (item) => item.featureId === featureId
  )
  if (fallback) {
    return fallback
  }

  return {
    featureId,
    state: 'disabled',
    reason: 'This Videorc feature is not enabled.'
  }
}

export function isFeatureEntitled(
  snapshot: EntitlementsSnapshot | null,
  featureId: FeatureId
): boolean {
  return entitlementCapability(snapshot, featureId).state !== 'disabled'
}

export function entitlementDisabledReason(
  snapshot: EntitlementsSnapshot | null,
  featureId: FeatureId
): string | null {
  const capability = entitlementCapability(snapshot, featureId)
  if (capability.state !== 'disabled') {
    return null
  }

  return capability.reason ?? 'This Videorc feature is not enabled.'
}
