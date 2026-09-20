// Public model bytes only. Sequential bounded reads avoid competing transfers.
// This is a local link sample, not a claim of globally fastest full downloads.
import { writeFile, mkdir } from 'node:fs/promises'
import { performance } from 'node:perf_hooks'
const models = [
  ['Qwen3-ASR', 'voconly/Qwen3-ASR-1.7B-gguf', 'voconly-org/Qwen3-ASR-1.7B-gguf', 'Qwen3-ASR-1.7B-Q5_K_M.gguf', 'd7aa4b50af3b672e3a5a2782953a823a9332e5b7'],
  ['Cohere', 'voconly/cohere-transcribe-03-2026-gguf', 'voconly-org/cohere-transcribe-03-2026-gguf', 'cohere-transcribe-03-2026-Q5_K_M.gguf', '0452067461a8df51e2245dd81f0122739caf424f'],
  ['Nemotron', 'voconly/nemotron-3.5-asr-streaming-0.6b-gguf', 'voconly-org/nemotron-3.5-asr-streaming-0.6b-gguf', 'nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf', '85c784fe0a42833abb5bd9e44c43980a0db46fe8'],
  ['Parakeet', 'voconly/parakeet-unified-en-0.6b-gguf', 'voconly-org/parakeet-unified-en-0.6b-gguf', 'parakeet-unified-en-0.6b-Q8_0.gguf', '598c2267a9bae5e6daf3c3237a44d272d11b7880'],
  ...['ggml-tiny.bin', 'ggml-base.bin', 'ggml-small-q5_1.bin', 'ggml-large-v3-turbo-q5_0.bin'].map(f => [f, 'cjc1887415157/whisper.cpp', 'ggerganov/whisper.cpp', f, 'master']),
  ['FunASR-encoder', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'funasr-encoder-f16.gguf', 'master'],
  ['FunASR-decoder', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'FunAudioLLM/Fun-ASR-Nano-GGUF', 'qwen3-0.6b-q4km.gguf', 'master'],
  ['FunASR-vad', 'FunAudioLLM/fsmn-vad-GGUF', 'FunAudioLLM/fsmn-vad-GGUF', 'fsmn-vad.gguf', 'master'],
]
const candidates = models.flatMap(([model, ms, hf, file, rev]) => [
  {model, source:'ModelScope', url:`https://modelscope.cn/models/${ms}/resolve/${rev}/${file}`},
  {model, source:'HF Mirror', url:`https://hf-mirror.com/${hf}/resolve/main/${file}`},
  {model, source:'Hugging Face', url:`https://huggingface.co/${hf}/resolve/main/${file}`},
])
const tar='https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2'
for(const [source,prefix] of [['GitHub',''],['GH llkk','https://gh.llkk.cc/'],['MoeYY','https://github.moeyy.xyz/'],['GHFast','https://ghfast.top/']]) candidates.push({model:'SenseVoice archive',source,url:prefix+tar})
const limit=2*1024*1024
const results=[]
await mkdir('.model-validation/source-benchmark', {recursive:true})
for (let pass=1;pass<=Number(process.env.BENCH_PASSES || 2);pass++) {
  for(const entry of candidates.filter(x=>!process.env.BENCH_FILTER || new RegExp(process.env.BENCH_FILTER).test(x.model))) {
    const controller=new AbortController(), timeout=setTimeout(()=>controller.abort(),10000)
    const start=performance.now();let bytes=0,firstByteMs=null,status=null,range=null,magic=''
    let error=null
    try {
      const response=await fetch(entry.url, {headers:{Range:`bytes=0-${limit-1}`},signal:controller.signal})
      status=response.status;range=response.headers.get('content-range')
      if(!response.ok) throw new Error(`HTTP ${status}`)
      const reader=response.body.getReader()
      while(bytes<limit) {
        const {value,done}=await reader.read();if(done)break
        if(firstByteMs===null){firstByteMs=Math.round(performance.now()-start);magic=Buffer.from(value.subarray(0,8)).toString('hex')}
        bytes+=value.length
      }
      await reader.cancel()
    } catch(e) {error=String(e.message)} finally {clearTimeout(timeout)}
    const seconds=(performance.now()-start)/1000
    const result={...entry,pass,status,range,bytes,seconds:Number(seconds.toFixed(3)),firstByteMs,MiBps:Number((bytes/1048576/seconds).toFixed(3)),magic,error}
    results.push(result); console.log(JSON.stringify(result))
    await writeFile('.model-validation/source-benchmark/results.json',JSON.stringify({date:new Date().toISOString(),method:'Sequential capped 2MiB Range GET; 10s timeout; includes connection time. No configured proxy. Results are local samples, not whole-file/global speed guarantees.',results},null,2))
  }
}
