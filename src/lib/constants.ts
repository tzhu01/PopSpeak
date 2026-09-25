// App metadata
export const APP_NAME = 'PopSpeak'
export const APP_VERSION = 'v0.4.4'
export const APP_REPO_URL = 'https://github.com/tzhu01/PopSpeak'
export const APP_LICENSE_URL = 'https://github.com/tzhu01/PopSpeak/blob/main/LICENSE'
// Cloud API base URL — defaults to www.popspeak.com but can be overridden via VITE_API_BASE_URL env var.
// All core features (BYOK mode) work without any cloud connection.
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'https://www.popspeak.com'

export const FREE_PLAN = {
  sttMinutes: 0,
  llmTokens: 0,
} as const

export const PRO_PLAN = {
  price: '¥9.9（建议价）',
  period: '年',
  features: [
    { label: '自愿支持开发', detail: '不限制免费离线功能' },
    { label: '云端按量购买', detail: '不承诺低价无限额度' },
    { label: '实际权益', detail: '以服务器已上架商品为准' },
  ],
} as const

export const STT_PROVIDERS = [
  { value: 'sensevoice', label: 'SenseVoice Small INT8' },
  { value: 'funasr-nano', label: 'Fun-ASR-Nano GGUF' },
  { value: 'local-whisper', label: 'OpenAI Whisper' },
  { value: 'volcengine-seedasr', label: '豆包 SeedASR 2.0（自费 API）' },
  { value: 'custom-whisper', label: '自定义云端接口' },
] as const

export const POLISH_MODES = [
  { value: 'fast', label: '轻度整理 — 去口癖、重复字与明显错字' },
  { value: 'deep', label: '深度润色 — 调整句式与书面表达' },
] as const

export const LLM_PROVIDERS = [
  { value: 'local-llama', label: '本地文字润色' },
  { value: 'ollama', label: 'Ollama（外部本地服务）' },
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'cloud', label: 'PopSpeak Cloud' },
] as const

export const LLM_DEFAULT_CONFIG: Record<string, { baseUrl: string; model: string }> = {
  zhipu: { baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4-flash' },
  deepseek: { baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  siliconflow: { baseUrl: 'https://api.siliconflow.cn/v1', model: 'Qwen/Qwen2.5-7B-Instruct' },
  openai: { baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  gemini: {
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
    model: 'gemini-2.0-flash',
  },
  moonshot: { baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  qwen: { baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-turbo' },
  groq: { baseUrl: 'https://api.groq.com/openai/v1', model: 'llama-3.3-70b-versatile' },
  claude: { baseUrl: 'https://openrouter.ai/api/v1', model: 'anthropic/claude-sonnet-4' },
  ollama: { baseUrl: 'http://localhost:11434/v1', model: 'llama3.2' },
  'local-llama': { baseUrl: 'http://127.0.0.1:11434/v1', model: 'qwen2.5-0.5b-instruct' },
  openrouter: { baseUrl: 'https://openrouter.ai/api/v1', model: 'openai/gpt-4o-mini' },
  cloud: { baseUrl: `${API_BASE_URL}/api/proxy`, model: 'default' },
}

export const LANGUAGES = [
  { value: 'multi', label: 'Auto Detect' },
  { value: 'zh', label: '中文普通话 (Mandarin)' },
  { value: 'yue', label: '粤语 (Cantonese)' },
  { value: 'wuu', label: '吴语/上海话 (Shanghainese)' },
  { value: 'hak', label: '客家话 (Hakka)' },
  { value: 'nan', label: '闽南语/台语 (Hokkien)' },
  { value: 'cdo', label: '闽东话 (Mindong)' },
  { value: 'hsn', label: '湘语 (Xiang)' },
  { value: 'gan', label: '赣语 (Gan)' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語 (Japanese)' },
  { value: 'ko', label: '한국어 (Korean)' },
  { value: 'fr', label: 'Français (French)' },
  { value: 'de', label: 'Deutsch (German)' },
  { value: 'es', label: 'Español (Spanish)' },
  { value: 'pt', label: 'Português (Portuguese)' },
  { value: 'ru', label: 'Русский (Russian)' },
  { value: 'ar', label: 'العربية (Arabic)' },
  { value: 'hi', label: 'हिन्दी (Hindi)' },
  { value: 'th', label: 'ไทย (Thai)' },
  { value: 'vi', label: 'Tiếng Việt (Vietnamese)' },
  { value: 'it', label: 'Italiano (Italian)' },
  { value: 'nl', label: 'Nederlands (Dutch)' },
  { value: 'tr', label: 'Türkçe (Turkish)' },
  { value: 'pl', label: 'Polski (Polish)' },
  { value: 'uk', label: 'Українська (Ukrainian)' },
  { value: 'id', label: 'Bahasa Indonesia' },
  { value: 'ms', label: 'Bahasa Melayu (Malay)' },
] as const

export const TARGET_LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'zh', label: '中文 (Chinese)' },
  { value: 'ja', label: '日本語 (Japanese)' },
  { value: 'ko', label: '한국어 (Korean)' },
  { value: 'fr', label: 'Français (French)' },
  { value: 'de', label: 'Deutsch (German)' },
  { value: 'es', label: 'Español (Spanish)' },
  { value: 'pt', label: 'Português (Portuguese)' },
  { value: 'ru', label: 'Русский (Russian)' },
  { value: 'ar', label: 'العربية (Arabic)' },
  { value: 'hi', label: 'हिन्दी (Hindi)' },
  { value: 'th', label: 'ไทย (Thai)' },
  { value: 'vi', label: 'Tiếng Việt (Vietnamese)' },
  { value: 'it', label: 'Italiano (Italian)' },
  { value: 'nl', label: 'Nederlands (Dutch)' },
  { value: 'tr', label: 'Türkçe (Turkish)' },
  { value: 'pl', label: 'Polski (Polish)' },
  { value: 'uk', label: 'Українська (Ukrainian)' },
  { value: 'id', label: 'Bahasa Indonesia' },
  { value: 'ms', label: 'Bahasa Melayu (Malay)' },
] as const
