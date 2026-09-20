import { createAuthClient } from 'better-auth/client'
import { API_BASE_URL } from './constants'

const fetchWithToken: typeof fetch = (url, init) => {
  const token = localStorage.getItem('session_token')
  if (token) {
    const headers = new Headers(init?.headers)
    if (!headers.has('Authorization')) {
      headers.set('Authorization', `Bearer ${token}`)
    }
    return fetch(url, { ...init, headers })
  }
  return fetch(url, init)
}

export const authClient = createAuthClient({
  baseURL: API_BASE_URL,
  fetchOptions: {
    customFetchImpl: fetchWithToken,
  },
})
