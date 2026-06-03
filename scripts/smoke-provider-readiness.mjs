const strict = process.env.VIDEORC_SMOKE_REQUIRE_PROVIDER_READY === '1'
const ok = []

const providers = [
  {
    label: 'YouTube',
    clientIdVars: ['VIDEORC_YOUTUBE_CLIENT_ID', 'VIDEORC_BUNDLED_YOUTUBE_CLIENT_ID'],
    secretVars: ['VIDEORC_YOUTUBE_CLIENT_SECRET'],
    secretRequired: false,
    extraChecks: [
      {
        label: 'verified Live-enabled channel available',
        env: 'VIDEORC_SMOKE_YOUTUBE_CHANNEL_READY'
      }
    ]
  },
  {
    label: 'Twitch',
    clientIdVars: ['VIDEORC_TWITCH_CLIENT_ID', 'VIDEORC_BUNDLED_TWITCH_CLIENT_ID'],
    secretVars: ['VIDEORC_TWITCH_CLIENT_SECRET'],
    secretRequired: true,
    extraChecks: [
      {
        label: 'test broadcaster account available',
        env: 'VIDEORC_SMOKE_TWITCH_ACCOUNT_READY'
      }
    ]
  },
  {
    label: 'X',
    clientIdVars: ['VIDEORC_X_CLIENT_ID', 'VIDEORC_BUNDLED_X_CLIENT_ID'],
    secretVars: ['VIDEORC_X_CLIENT_SECRET'],
    secretRequired: false,
    extraChecks: [
      {
        label: 'native live partner/API access available',
        env: 'VIDEORC_SMOKE_X_NATIVE_LIVE_ACCESS'
      }
    ]
  }
]

const failures = []

for (const provider of providers) {
  const clientId = firstPresent(provider.clientIdVars)
  const secret = firstPresent(provider.secretVars)
  const missing = []
  if (!clientId) {
    missing.push(`one of ${provider.clientIdVars.join(', ')}`)
  }
  if (provider.secretRequired && !secret) {
    missing.push(provider.secretVars.join(' or '))
  }
  for (const check of provider.extraChecks) {
    if (process.env[check.env] !== '1') {
      missing.push(`${check.env}=1 (${check.label})`)
    }
  }

  if (missing.length) {
    failures.push(`${provider.label}: missing ${missing.join('; ')}`)
    console.log(`[missing] ${provider.label}`)
    for (const item of missing) {
      console.log(`  - ${item}`)
    }
  } else {
    ok.push(provider.label)
    console.log(`[ready] ${provider.label}`)
  }
}

if (ok.length) {
  console.log('')
  console.log(`Ready providers: ${ok.join(', ')}`)
}

if (failures.length) {
  console.log('')
  console.log('Provider live-smoke readiness is incomplete:')
  for (const failure of failures) {
    console.log(`- ${failure}`)
  }
  if (strict) {
    process.exitCode = 1
  } else {
    console.log('')
    console.log('Set VIDEORC_SMOKE_REQUIRE_PROVIDER_READY=1 to make missing provider prerequisites fail.')
  }
} else {
  console.log('')
  console.log('Provider live-smoke readiness OK.')
}

function firstPresent(names) {
  return names.find((name) => typeof process.env[name] === 'string' && process.env[name].trim().length > 0)
}
