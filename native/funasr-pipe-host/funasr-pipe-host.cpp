// PopSpeak persistent Fun-ASR-Nano host for Windows.
//
// The upstream FunASR llama.cpp CLI owns the encoder/LLM implementation but
// intentionally loads both models for every command invocation.  This host is
// built in the same translation unit as that implementation, then keeps the
// encoder weights, llama_model and llama_context alive while serving raw PCM16
// requests over a local named pipe.

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <exception>
#include <stdexcept>
#include <string>
#include <vector>

// Reuse the exact, upstream-validated encoder and decoder implementation.  The
// build script supplies this file's directory after the pinned upstream source
// directory, so this include resolves to FunASR's funasr-cli.cpp.
#define main funasr_original_cli_main
#include "funasr-cli.cpp"
#undef main

#ifndef _WIN32
#error "PopSpeak's FunASR named-pipe host is currently Windows-only"
#endif

namespace {

constexpr std::uint32_t REQUEST_MAGIC = 0x41465350;  // "PSFA" little-endian
constexpr std::uint32_t RESPONSE_MAGIC = 0x52465350; // "PSFR" little-endian
constexpr std::uint16_t PROTOCOL_VERSION = 2;
constexpr std::uint16_t FLAG_USE_VAD = 0x0001;
constexpr std::uint16_t FLAG_SHUTDOWN = 0x8000;
constexpr std::uint64_t MAX_PCM_BYTES = 10ULL * 1024ULL * 1024ULL;

#pragma pack(push, 1)
struct RequestHeader {
    std::uint32_t magic;
    std::uint16_t version;
    std::uint16_t flags;
    std::uint32_t sample_rate;
    std::uint64_t request_id;
    std::uint64_t pcm_bytes;
    std::uint32_t hotword_bytes;
};

struct ResponseHeader {
    std::uint32_t magic;
    std::uint16_t version;
    std::uint16_t status;
    std::uint64_t request_id;
    std::uint32_t text_bytes;
    std::uint32_t elapsed_ms;
};
#pragma pack(pop)

static_assert(sizeof(RequestHeader) == 32, "pipe request ABI changed");
static_assert(sizeof(ResponseHeader) == 24, "pipe response ABI changed");

std::wstring widen_ascii(const std::string & value) {
    return std::wstring(value.begin(), value.end());
}

void read_exact(HANDLE pipe, void * destination, std::size_t bytes) {
    auto * cursor = static_cast<std::uint8_t *>(destination);
    while (bytes > 0) {
        DWORD received = 0;
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(bytes, 1U << 20));
        if (!ReadFile(pipe, cursor, chunk, &received, nullptr) || received == 0) {
            throw std::runtime_error("named pipe closed while reading request");
        }
        cursor += received;
        bytes -= received;
    }
}

void write_exact(HANDLE pipe, const void * source, std::size_t bytes) {
    const auto * cursor = static_cast<const std::uint8_t *>(source);
    while (bytes > 0) {
        DWORD sent = 0;
        const DWORD chunk = static_cast<DWORD>(std::min<std::size_t>(bytes, 1U << 20));
        if (!WriteFile(pipe, cursor, chunk, &sent, nullptr) || sent == 0) {
            throw std::runtime_error("named pipe closed while writing response");
        }
        cursor += sent;
        bytes -= sent;
    }
}

// The application sends an already bounded vocabulary. Validate the pipe
// independently so malformed clients cannot inject ChatML or exhaust context.
std::string hotword_prompt(const std::string & payload) {
    if (payload.empty()) return "语音转写：";
    std::vector<std::string> words;
    std::size_t start = 0;
    int budget = 0;
    while (start < payload.size()) {
        const auto end = payload.find('\n', start);
        const auto word = payload.substr(start, end == std::string::npos ? end : end - start);
        if (word.empty()) throw std::runtime_error("empty hotword");
        const int wide_size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
            word.data(), static_cast<int>(word.size()), nullptr, 0);
        if (wide_size <= 0) throw std::runtime_error("invalid hotword UTF-8");
        std::wstring wide(static_cast<std::size_t>(wide_size), L'\0');
        MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, word.data(),
            static_cast<int>(word.size()), wide.data(), wide_size);
        int count = 0;
        int cost = 2;
        for (std::size_t i = 0; i < wide.size(); ++i) {
            const unsigned int code = wide[i];
            if (code <= 31 || (code >= 127 && code <= 159) ||
                std::wstring(L"<>[]{}|,").find(wide[i]) != std::wstring::npos) {
                throw std::runtime_error("hotword contains a reserved character");
            }
            if (code >= 0xD800 && code <= 0xDBFF) ++i;
            ++count;
            cost += code <= 127 ? 1 : 2;
        }
        if (count > 32 || words.size() >= 32 || budget + cost > 160) {
            throw std::runtime_error("hotword budget exceeded");
        }
        if (std::find(words.begin(), words.end(), word) != words.end()) {
            throw std::runtime_error("duplicate hotword");
        }
        words.push_back(word);
        budget += cost;
        if (end == std::string::npos) break;
        start = end + 1;
    }
    std::string prompt = "请结合上下文信息，更加准确地完成语音转写任务。如果没有相关信息，我们会留空。\n\n\n**上下文信息：**\n\n\n热词列表：[";
    for (std::size_t i = 0; i < words.size(); ++i) {
        if (i) prompt += ", ";
        prompt += words[i];
    }
    return prompt + "]\n语音转写：";
}

struct Runtime {
    enc_model encoder;
    llama_model * model = nullptr;
    llama_context * context = nullptr;
    llama_sampler * sampler = nullptr;
    const llama_vocab * vocab = nullptr;
    std::vector<llama_token> suffix;
    std::string vad_path;
    int prediction_limit = 512;
    int vad_max_segment_ms = 30000;

    ~Runtime() {
        if (sampler) llama_sampler_free(sampler);
        if (context) llama_free(context);
        if (model) llama_model_free(model);
        if (encoder.ctx_w) ggml_free(encoder.ctx_w);
    }

    void load(const std::string & encoder_path,
              const std::string & model_path,
              const std::string & configured_vad_path,
              int threads,
              float repetition_penalty) {
        ggml_time_init();
        if (!load_enc(encoder_path.c_str(), encoder)) {
            throw std::runtime_error("failed to load FunASR encoder");
        }

        ggml_backend_load_all();
        llama_model_params model_params = llama_model_default_params();
        model_params.n_gpu_layers = 0;
        model = llama_model_load_from_file(model_path.c_str(), model_params);
        if (!model) throw std::runtime_error("failed to load Qwen3 ASR decoder");

        vocab = llama_model_get_vocab(model);
        llama_context_params context_params = llama_context_default_params();
        context_params.n_ctx = 2048;
        context_params.n_batch = 2048;
        context_params.n_ubatch = 2048;
        context = llama_init_from_model(model, context_params);
        if (!context) throw std::runtime_error("failed to create Qwen3 context");
        llama_set_n_threads(context, std::max(1, threads), std::max(1, threads));

        auto chain_params = llama_sampler_chain_default_params();
        sampler = llama_sampler_chain_init(chain_params);
        if (repetition_penalty != 1.0f) {
            llama_sampler_chain_add(
                sampler,
                llama_sampler_init_penalties(
                    llama_vocab_n_tokens(vocab), 256, repetition_penalty, 0.0f, 0.0f));
        }
        llama_sampler_chain_add(sampler, llama_sampler_init_greedy());

        const char * suffix_text = "<|im_end|>\n<|im_start|>assistant\n";
        suffix = tokenize(suffix_text);
        vad_path = configured_vad_path;
    }

    std::vector<llama_token> tokenize(const char * text) const {
        const int bytes = static_cast<int>(std::strlen(text));
        const int count = -llama_tokenize(vocab, text, bytes, nullptr, 0, false, true);
        if (count <= 0) throw std::runtime_error("failed to tokenize FunASR prompt");
        std::vector<llama_token> tokens(static_cast<std::size_t>(count));
        if (llama_tokenize(vocab, text, bytes, tokens.data(), count, false, true) < 0) {
            throw std::runtime_error("failed to tokenize FunASR prompt");
        }
        return tokens;
    }

    std::string transcribe(const std::vector<std::uint8_t> & pcm, bool use_vad,
                           const std::string & prompt) {
        if (pcm.size() < 800 || pcm.size() % 2 != 0) return {};

        const std::string request_prompt =
            "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n"
            "<|im_start|>user\n" + prompt;
        auto request_prefix = tokenize(request_prompt.c_str());
        if (request_prefix.size() > 384) throw std::runtime_error("hotword prompt is too long");

        std::vector<float> waveform(pcm.size() / 2);
        for (std::size_t index = 0; index < waveform.size(); ++index) {
            const auto lo = static_cast<std::uint16_t>(pcm[index * 2]);
            const auto hi = static_cast<std::uint16_t>(pcm[index * 2 + 1]);
            const auto sample = static_cast<std::int16_t>(lo | (hi << 8));
            waveform[index] = static_cast<float>(sample) / 32768.0f;
        }

        std::vector<std::pair<int, int>> windows;
        if (use_vad && !vad_path.empty()) {
            std::vector<std::pair<int, int>> segments_ms;
            if (!funasr_vad_segments(vad_path, waveform, vad_max_segment_ms, segments_ms)) {
                throw std::runtime_error("FSMN-VAD failed");
            }
            for (const auto & segment : segments_ms) {
                const int offset = static_cast<int>(
                    static_cast<std::int64_t>(segment.first) * 16000 / 1000);
                int end = static_cast<int>(
                    static_cast<std::int64_t>(segment.second) * 16000 / 1000);
                end = std::min(end, static_cast<int>(waveform.size()));
                if (end > offset) windows.push_back({offset, end - offset});
            }
        } else {
            windows.push_back({0, static_cast<int>(waveform.size())});
        }

        std::string transcript;
        for (const auto & window : windows) {
            const int offset = window.first;
            const int length = window.second;
            if (length < WINLEN) continue;

            std::vector<float> segment(
                waveform.begin() + offset, waveform.begin() + offset + length);
            int frame_count = 0;
            auto features = compute_fbank(std::move(segment), frame_count);
            int embedding_size = 0;
            auto audio_embeddings =
                run_encoder(encoder, std::move(features), frame_count, 560, embedding_size);
            int output_length = 1 + (frame_count - 3 + 2) / 2;
            output_length = 1 + (output_length - 3 + 2) / 2;
            const int audio_tokens = (output_length - 1) / 2 + 1;

            llama_memory_clear(llama_get_memory(context), true);
            llama_sampler_reset(sampler);
            int past = 0;
            if (decode_batch(context, static_cast<int>(request_prefix.size()), request_prefix.data(), nullptr,
                             0, past, false) != 0 ||
                decode_batch(context, audio_tokens, nullptr, audio_embeddings.data(),
                             embedding_size, past, false) != 0 ||
                decode_batch(context, static_cast<int>(suffix.size()), suffix.data(), nullptr,
                             0, past, true) != 0) {
                throw std::runtime_error("Qwen3 prompt decode failed");
            }

            std::string segment_text;
            llama_token token = llama_sampler_sample(sampler, context, -1);
            for (int index = 0; index < prediction_limit; ++index) {
                if (llama_vocab_is_eog(vocab, token)) break;
                char piece[256];
                const int count =
                    llama_token_to_piece(vocab, token, piece, sizeof(piece), 0, true);
                if (count > 0) segment_text.append(piece, static_cast<std::size_t>(count));
                llama_sampler_accept(sampler, token);
                if (decode_batch(context, 1, &token, nullptr, 0, past, true) != 0) {
                    throw std::runtime_error("Qwen3 token decode failed");
                }
                token = llama_sampler_sample(sampler, context, -1);
            }
            if (segment_text != "/sil") transcript += segment_text;
        }
        return transcript;
    }
};

void send_response(HANDLE pipe,
                   std::uint64_t request_id,
                   std::uint16_t status,
                   const std::string & text,
                   std::uint32_t elapsed_ms) {
    if (text.size() > UINT32_MAX) throw std::runtime_error("response too large");
    const ResponseHeader header{
        RESPONSE_MAGIC,
        PROTOCOL_VERSION,
        status,
        request_id,
        static_cast<std::uint32_t>(text.size()),
        elapsed_ms,
    };
    write_exact(pipe, &header, sizeof(header));
    if (!text.empty()) write_exact(pipe, text.data(), text.size());
    FlushFileBuffers(pipe);
}

int serve(const std::string & pipe_name, Runtime & runtime) {
    const std::wstring wide_pipe = widen_ascii(pipe_name);
    bool stopping = false;
    while (!stopping) {
        HANDLE pipe = CreateNamedPipeW(
            wide_pipe.c_str(), PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1, 64 * 1024, 64 * 1024, 0, nullptr);
        if (pipe == INVALID_HANDLE_VALUE) {
            std::fprintf(stderr, "[pipe] CreateNamedPipeW failed: %lu\n", GetLastError());
            return 1;
        }

        const BOOL connected = ConnectNamedPipe(pipe, nullptr)
            ? TRUE
            : (GetLastError() == ERROR_PIPE_CONNECTED);
        if (!connected) {
            CloseHandle(pipe);
            continue;
        }

        RequestHeader header{};
        try {
            read_exact(pipe, &header, sizeof(header));
            if (header.magic != REQUEST_MAGIC || header.version != PROTOCOL_VERSION) {
                throw std::runtime_error("unsupported PopSpeak FunASR pipe protocol");
            }
            if (header.sample_rate != 16000) {
                throw std::runtime_error("FunASR pipe accepts 16 kHz PCM only");
            }
            if (header.pcm_bytes > MAX_PCM_BYTES || header.pcm_bytes % 2 != 0) {
                throw std::runtime_error("invalid PCM payload size");
            }

            if (header.hotword_bytes > 4096) throw std::runtime_error("hotword payload too large");
            std::string hotwords(header.hotword_bytes, '\0');
            if (!hotwords.empty()) read_exact(pipe, hotwords.data(), hotwords.size());
            const std::string prompt = hotword_prompt(hotwords);

            std::vector<std::uint8_t> pcm(static_cast<std::size_t>(header.pcm_bytes));
            if (!pcm.empty()) read_exact(pipe, pcm.data(), pcm.size());
            const auto started = std::chrono::steady_clock::now();
            std::string text;
            if ((header.flags & FLAG_SHUTDOWN) != 0) {
                stopping = true;
            } else if (!pcm.empty()) {
                text = runtime.transcribe(pcm, (header.flags & FLAG_USE_VAD) != 0, prompt);
            }
            const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                std::chrono::steady_clock::now() - started);
            send_response(pipe, header.request_id, 0, text,
                          static_cast<std::uint32_t>(elapsed.count()));
        } catch (const std::exception & error) {
            try {
                send_response(pipe, header.request_id, 1, error.what(), 0);
            } catch (...) {
            }
            std::fprintf(stderr, "[pipe] request failed: %s\n", error.what());
        }
        DisconnectNamedPipe(pipe);
        CloseHandle(pipe);
    }
    return 0;
}

void usage(const char * executable) {
    std::fprintf(stderr,
        "usage: %s --enc encoder.gguf -m qwen3.gguf --pipe \\\\.\\pipe\\name "
        "[--vad fsmn-vad.gguf] [--threads N] [-n npred] [--rep R]\n",
        executable);
}

} // namespace

int main(int argc, char ** argv) {
    std::string encoder_path;
    std::string model_path;
    std::string vad_path;
    std::string pipe_name;
    int threads = 8;
    int prediction_limit = 512;
    float repetition_penalty = 1.0f;

    for (int index = 1; index < argc; ++index) {
        if (!std::strcmp(argv[index], "--enc") && index + 1 < argc) {
            encoder_path = argv[++index];
        } else if (!std::strcmp(argv[index], "-m") && index + 1 < argc) {
            model_path = argv[++index];
        } else if (!std::strcmp(argv[index], "--vad") && index + 1 < argc) {
            vad_path = argv[++index];
        } else if (!std::strcmp(argv[index], "--pipe") && index + 1 < argc) {
            pipe_name = argv[++index];
        } else if (!std::strcmp(argv[index], "--threads") && index + 1 < argc) {
            threads = std::max(1, std::atoi(argv[++index]));
        } else if (!std::strcmp(argv[index], "-n") && index + 1 < argc) {
            prediction_limit = std::max(1, std::atoi(argv[++index]));
        } else if (!std::strcmp(argv[index], "--rep") && index + 1 < argc) {
            repetition_penalty = static_cast<float>(std::atof(argv[++index]));
        } else {
            usage(argv[0]);
            return 1;
        }
    }

    if (encoder_path.empty() || model_path.empty() || pipe_name.empty()) {
        usage(argv[0]);
        return 1;
    }

    try {
        const auto started = std::chrono::steady_clock::now();
        Runtime runtime;
        runtime.prediction_limit = prediction_limit;
        runtime.load(encoder_path, model_path, vad_path, threads, repetition_penalty);
        const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - started);
        std::fprintf(stderr, "[ready] models loaded in %lldms; pipe=%s\n",
                     static_cast<long long>(elapsed.count()), pipe_name.c_str());
        return serve(pipe_name, runtime);
    } catch (const std::exception & error) {
        std::fprintf(stderr, "[fatal] %s\n", error.what());
        return 1;
    }
}
