import { onOpenUrl } from '@tauri-apps/plugin-deep-link'
import { useAuthStore } from '../stores/authStore'
import { exchangeOAuthCode } from './account-service'

/** Pending OAuth state for CSRF validation. */
let pendingOAuthState: string | null = null
let pendingOAuthVerifier: string | null = null
let pendingOAuthTimer: ReturnType<typeof setTimeout> | null = null

/** Generate and store a random state string for OAuth CSRF protection. */
export function generateOAuthState(): string {
  clearOAuthState()
  const state = crypto.randomUUID()
  pendingOAuthState = state
  // Auto-expire after 5 minutes to prevent stale state
  pendingOAuthTimer = setTimeout(clearOAuthState, 5 * 60 * 1000)
  return state
}

/** Clear pending OAuth state (e.g. user cancelled or timed out). */
export function clearOAuthState(): void {
  pendingOAuthState = null
  pendingOAuthVerifier = null
  if (pendingOAuthTimer) {
    clearTimeout(pendingOAuthTimer)
    pendingOAuthTimer = null
  }
}

/** Bind a one-time desktop authorization code to this process using PKCE. */
export async function generateOAuthRequest(): Promise<{ state: string; codeChallenge: string }> {
  const state = generateOAuthState()
  const base64url = (bytes: Uint8Array) =>
    btoa(String.fromCharCode(...bytes))
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/, '')
  const verifier = base64url(crypto.getRandomValues(new Uint8Array(32)))
  const challenge = base64url(
    new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))),
  )
  if (pendingOAuthState !== state) throw new Error('登录请求已失效，请重试。')
  pendingOAuthVerifier = verifier
  return { state, codeChallenge: challenge }
}

export async function initDeepLinkListener() {
  try {
    await onOpenUrl(async (urls) => {
      for (const rawUrl of urls) {
        await handleDeepLinkUrl(rawUrl)
      }
    })
  } catch {
    // Deep link plugin not available (e.g. web dev mode)
  }
}

/** Basic sanity check: token must be a non-empty alphanumeric/JWT-like string. */
function isValidToken(token: string): boolean {
  return /^[\w\-._~+/]+=*$/.test(token) && token.length >= 10 && token.length <= 4096
}

export async function handleDeepLinkUrl(rawUrl: string) {
  let url: URL
  try {
    url = new URL(rawUrl)
  } catch {
    return
  }

  // Only accept our custom scheme
  if (url.protocol !== 'popspeak:') return

  const path = url.hostname + url.pathname
  const params = url.searchParams

  // Only one-time codes travel in URLs; access tokens never belong in a deep link.
  if (path === 'auth/callback' || path === 'auth/callback/') {
    const code = params.get('code')
    const state = params.get('state')

    // Reject tokens when no OAuth flow was initiated (prevents external injection)
    if (!pendingOAuthState || !pendingOAuthVerifier) {
      return
    }
    // Validate CSRF state
    if (state !== pendingOAuthState) {
      return
    }
    const verifier = pendingOAuthVerifier
    clearOAuthState()
    if (code && isValidToken(code)) {
      try {
        const result = await exchangeOAuthCode(code, state!, verifier)
        if (!isValidToken(result.token)) throw new Error('Invalid session')
        await useAuthStore.getState().handleDeepLinkToken(result.token)
      } catch {
        useAuthStore.setState({ error: '登录未完成，请重新发起登录。' })
      }
    }
    window.location.hash = '#/account'
    return
  }

  // popspeak://checkout/success
  if (path === 'checkout/success' || path === 'checkout/success/') {
    await useAuthStore.getState().refreshSubscription()
    window.location.hash = '#/account'
    return
  }
}
