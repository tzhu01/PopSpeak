// Public model transport checks only. Does not use accounts, credentials or proxies.
import { mkdir, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { performance } from 'node:perf_hooks'

const out = process.env.PROBE_OUTPUT || '.model-validation/legacy-fallback-probe'
await mkdir(out, { recursive: true })
const sense = 'csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17'
const senseMs = 'fengge2024/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17'
const specs = [
  ['sensevoice-model', senseMs, sense, 'model.int8.onnx', 'c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51'],
  ['sensevoice-tokens', senseMs, sense, 'tokens.txt', 'f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc'],
  ['funasr-encoder', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'funasr-encoder-f16.gguf', 'f92f91d01a24fbed6c863495b2ee8c6a6788144a02858b75743f0946668de8a2'],
  ['funasr-q4', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'qwen3-0.6b-q4km.gguf', 'cc5057552aa9dddedcda73ea8889854e8a257eb07d0a561b7234465c1e856f22'],
  ['funasr-vad', 'FunAudioLLM/fsmn-vad-GGUF', 'FunAudioLLM/fsmn-vad-GGUF', 'fsmn-vad.gguf', '1270f2559c495f4e7b6e739541151027d360761a3fda43fc147034f5719f5479'],
  ['whisper-tiny', 'cjc1887415157/whisper.cpp', 'ggerganov/whisper.cpp', 'ggml-tiny.bin', 'be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21'],
  ['whisper-base', 'cjc1887415157/whisper.cpp', 'ggerganov/whisper.cpp', 'ggml-base.bin', '60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe'],
  ['whisper-small', 'cjc1887415157/whisper.cpp', 'ggerganov/whisper.cpp', 'ggml-small-q5_1.bin', 'ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb'],
  ['whisper-turbo', 'timeless/whispercpp', 'ggerganov/whisper.cpp', 'ggml-large-v3-turbo-q5_0.bin', '394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2'],
]
const metadata = []
for (const repo of process.env.PROBE_SKIP_METADATA ? [] : [...new Set(specs.map((entry) => entry[2]))]) {
  for (const host of ['https://hf-mirror.com', 'https://huggingface.co']) {
    const url = `${host}/api/models/${repo}/tree/main?recursive=true`
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(12000) })
      const body = await response.json()
      metadata.push({ url, status: response.status, body })
    } catch (error) {
      metadata.push({ url, error: String(error.message) })
    }
    await writeFile(`${out}/metadata.json`, JSON.stringify(metadata, null, 2))
  }
}
for (const repo of process.env.PROBE_SKIP_METADATA ? [] : [...new Set(specs.map((entry) => entry[1]))]) {
  const url = `https://modelscope.cn/api/v1/models/${repo}/repo/files?Revision=master&Recursive=true`
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(12000) })
    const body = await response.json()
    metadata.push({ url, status: response.status, body })
  } catch (error) {
    metadata.push({ url, error: String(error.message) })
  }
  await writeFile(`${out}/metadata.json`, JSON.stringify(metadata, null, 2))
}
const revision = {
  [senseMs]: '9bd9398d89294cbf1964af126ff13e6890394cbc',
  'FunAudioLLM/Fun-ASR-Nano-GGUF': '51dcf4922439c10e0c2e59bc99be8a343d2fe71f',
  'FunAudioLLM/fsmn-vad-GGUF': 'f04fc3013641c8d59c156e2cbf171c1ad596f74d',
  'cjc1887415157/whisper.cpp': 'ac12dbec310c2fd6e67398e808c40d80210ce4d0',
  'timeless/whispercpp': 'f93b8669080e40afe671b275d8cd67fd2060c956',
}
const sources = specs.flatMap(([model, ms, hf, file, sha256]) => process.env.PROBE_EXTRA ? [
  { model, source: 'ModelScope pinned revision', url: `https://modelscope.cn/models/${ms}/resolve/${revision[ms]}/${file}`, sha256 },
  { model, source: 'ModelScope international', url: `https://modelscope.ai/models/${ms}/resolve/master/${file}`, sha256 },
] : [
  { model, source: 'ModelScope', url: `https://modelscope.cn/models/${ms}/resolve/master/${file}`, sha256 },
  { model, source: 'HF-Mirror', url: `https://hf-mirror.com/${hf}/resolve/main/${file}`, sha256 },
  { model, source: 'HuggingFace', url: `https://huggingface.co/${hf}/resolve/main/${file}`, sha256 },
])
const results = []
const limit = Number(process.env.PROBE_BYTES || 2 * 1024 * 1024)
for (const entry of sources.filter((item) => !process.env.PROBE_FILTER || new RegExp(process.env.PROBE_FILTER).test(item.model))) {
  const started = performance.now()
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), 15000)
  let status = null, bytes = 0, firstByteMs = null, totalBytes = null, range = null, sampleSha256 = null, finalUrl = null, error = null
  const hash = createHash('sha256')
  try {
    const response = await fetch(entry.url, { headers: { Range: `bytes=0-${limit - 1}` }, signal: controller.signal })
    status = response.status
    finalUrl = new URL(response.url).origin + new URL(response.url).pathname
    range = response.headers.get('content-range')
    totalBytes = range ? Number(range.split('/').at(-1)) : Number(response.headers.get('content-length')) || null
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const reader = response.body.getReader()
    while (bytes < Math.min(limit, totalBytes || limit)) {
      const next = await reader.read()
      if (next.done) break
      if (firstByteMs === null) firstByteMs = Math.round(performance.now() - started)
      const remaining = limit - bytes
      const chunk = next.value.subarray(0, remaining)
      hash.update(chunk)
      bytes += chunk.length
    }
    await reader.cancel().catch(() => {})
    sampleSha256 = hash.digest('hex')
    if (totalBytes && totalBytes <= limit && bytes === totalBytes && sampleSha256 !== entry.sha256) {
      throw new Error('Full-file SHA256 mismatch')
    }
  } catch (failure) { error = String(failure.message) } finally { clearTimeout(timer) }
  const seconds = (performance.now() - started) / 1000
  const result = { ...entry, status, bytes, totalBytes, range, seconds: Number(seconds.toFixed(3)), firstByteMs, MiBps: Number((bytes / 1048576 / seconds).toFixed(3)), sampleSha256, fullHashVerified: sampleSha256 === entry.sha256, finalUrl, error }
  results.push(result)
  console.log(JSON.stringify(result))
  await writeFile(`${out}/results.json`, JSON.stringify({ date: new Date().toISOString(), method: 'Sequential bounded 2MiB Range GET, 15s timeout, no explicit proxy. Other independent tasks may use network; not a global speed guarantee.', results }, null, 2))
}
