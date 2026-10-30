// SHA-256 accelerated with the SHA extensions (SHA-NI) where available.
//
// The runtime probe hf_has_sha_ni() reports availability, combining both
// compile-time support (the compiler was built with -msha) and CPU support,
// so callers can fall back to their own portable implementations.

#include "hf_native.h"

#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
#define HF_X86 1
#if defined(_MSC_VER)
#include <intrin.h>
#else
#include <cpuid.h>
#endif
#include <immintrin.h>
#endif

#if defined(HF_X86) && defined(__SHA__)
#define HF_SHA_X86 1
#else
#define HF_SHA_X86 0
#endif

namespace {

uint32_t rotr32(uint32_t value, unsigned int amount) {
    return (value >> amount) | (value << (32u - amount));
}

uint32_t read_be32(const uint8_t* source) {
    return (static_cast<uint32_t>(source[0]) << 24) |
           (static_cast<uint32_t>(source[1]) << 16) |
           (static_cast<uint32_t>(source[2]) << 8) |
           static_cast<uint32_t>(source[3]);
}

void write_be32(uint8_t* target, uint32_t value) {
    target[0] = static_cast<uint8_t>((value >> 24) & 0xffu);
    target[1] = static_cast<uint8_t>((value >> 16) & 0xffu);
    target[2] = static_cast<uint8_t>((value >> 8) & 0xffu);
    target[3] = static_cast<uint8_t>(value & 0xffu);
}

const uint32_t K256[64] = {
    0x428a2f98u, 0x71374491u, 0xb5c0fbcfu, 0xe9b5dba5u,
    0x3956c25bu, 0x59f111f1u, 0x923f82a4u, 0xab1c5ed5u,
    0xd807aa98u, 0x12835b01u, 0x243185beu, 0x550c7dc3u,
    0x72be5d74u, 0x80deb1feu, 0x9bdc06a7u, 0xc19bf174u,
    0xe49b69c1u, 0xefbe4786u, 0x0fc19dc6u, 0x240ca1ccu,
    0x2de92c6fu, 0x4a7484aau, 0x5cb0a9dcu, 0x76f988dau,
    0x983e5152u, 0xa831c66du, 0xb00327c8u, 0xbf597fc7u,
    0xc6e00bf3u, 0xd5a79147u, 0x06ca6351u, 0x14292967u,
    0x27b70a85u, 0x2e1b2138u, 0x4d2c6dfcu, 0x53380d13u,
    0x650a7354u, 0x766a0abbu, 0x81c2c92eu, 0x92722c85u,
    0xa2bfe8a1u, 0xa81a664bu, 0xc24b8b70u, 0xc76c51a3u,
    0xd192e819u, 0xd6990624u, 0xf40e3585u, 0x106aa070u,
    0x19a4c116u, 0x1e376c08u, 0x2748774cu, 0x34b0bcb5u,
    0x391c0cb3u, 0x4ed8aa4au, 0x5b9cca4fu, 0x682e6ff3u,
    0x748f82eeu, 0x78a5636fu, 0x84c87814u, 0x8cc70208u,
    0x90befffau, 0xa4506cebu, 0xbef9a3f7u, 0xc67178f2u,
};

const uint32_t INITIAL_STATE[8] = {
    0x6a09e667u, 0xbb67ae85u, 0x3c6ef372u, 0xa54ff53au,
    0x510e527fu, 0x9b05688cu, 0x1f83d9abu, 0x5be0cd19u,
};

void process_block(uint32_t state[8], const uint8_t* data, size_t blocks) {
#if !HF_SHA_X86
    for (size_t block = 0; block < blocks; ++block, data += 64) {
        uint32_t w[64];
        for (int index = 0; index < 64; ++index) {
            w[index] = read_be32(data + index * 4);
        }
        for (int index = 16; index < 64; ++index) {
            const uint32_t s0 = rotr32(w[index - 15], 7) ^ rotr32(w[index - 15], 18) ^
                                (w[index - 15] >> 3);
            const uint32_t s1 = rotr32(w[index - 2], 17) ^ rotr32(w[index - 2], 19) ^
                                (w[index - 2] >> 10);
            w[index] = w[index - 16] + s0 + w[index - 7] + s1;
        }

        uint32_t a = state[0];
        uint32_t b = state[1];
        uint32_t c = state[2];
        uint32_t d = state[3];
        uint32_t e = state[4];
        uint32_t f = state[5];
        uint32_t g = state[6];
        uint32_t h = state[7];

        for (int index = 0; index < 64; ++index) {
            const uint32_t big_s0 = rotr32(a, 2) ^ rotr32(a, 13) ^ rotr32(a, 22);
            const uint32_t big_s1 = rotr32(e, 6) ^ rotr32(e, 11) ^ rotr32(e, 25);
            const uint32_t chosen = (e & f) ^ (~e & g);
            const uint32_t majority = (a & b) ^ (a & c) ^ (b & c);
            const uint32_t temp1 = h + big_s1 + chosen + K256[index] + w[index];
            const uint32_t temp2 = big_s0 + majority;
            h = g;
            g = f;
            f = e;
            e = d + temp1;
            d = c;
            c = b;
            b = a;
            a = temp1 + temp2;
        }

        state[0] += a;
        state[1] += b;
        state[2] += c;
        state[3] += d;
        state[4] += e;
        state[5] += f;
        state[6] += g;
        state[7] += h;
    }
#else
    __m128i state_abef;
    __m128i state_cdgh;
    __m128i message;
    __m128i temporary;
    __m128i words[4];
    const __m128i byte_mask =
        _mm_set_epi64x(0x0c0d0e0f08090a0bULL, 0x0405060700010203ULL);

    for (size_t block = 0; block < blocks; ++block, data += 64) {
        __m128i shuffled = _mm_loadu_si128(reinterpret_cast<const __m128i*>(state));
        __m128i upper = _mm_loadu_si128(
            reinterpret_cast<const __m128i*>(state + 4));

        shuffled = _mm_shuffle_epi32(shuffled, 0xb1);
        upper = _mm_shuffle_epi32(upper, 0x1b);
        state_abef = _mm_alignr_epi8(shuffled, upper, 8);
        state_cdgh = _mm_blend_epi16(upper, shuffled, 0xf0);

        const __m128i saved_abef = state_abef;
        const __m128i saved_cdgh = state_cdgh;

        for (int group = 0; group < 16; ++group) {
            const int round = group * 4;
            const int index = group & 3;

            if (group < 4) {
                words[index] = _mm_shuffle_epi8(
                    _mm_loadu_si128(reinterpret_cast<const __m128i*>(
                        data + group * 16)),
                    byte_mask);
            }

            const __m128i constants = _mm_set_epi64x(
                (static_cast<uint64_t>(K256[round + 3]) << 32) | K256[round + 2],
                (static_cast<uint64_t>(K256[round + 1]) << 32) | K256[round]);

            message = _mm_add_epi32(words[index], constants);
            state_cdgh =
                _mm_sha256rnds2_epu32(state_cdgh, state_abef, message);

            if (group >= 3 && group <= 14) {
                const int next = (index + 1) & 3;
                temporary = _mm_alignr_epi8(
                    words[index], words[(index + 3) & 3], 4);
                words[next] = _mm_add_epi32(words[next], temporary);
                words[next] = _mm_sha256msg2_epu32(words[next], words[index]);
            }

            message = _mm_shuffle_epi32(message, 0x0e);
            state_abef = _mm_sha256rnds2_epu32(state_abef, state_cdgh, message);

            if (group >= 1 && group <= 11) {
                words[(index + 3) & 3] = _mm_sha256msg1_epu32(
                    words[(index + 3) & 3], words[index]);
            }
        }

        state_abef = _mm_add_epi32(state_abef, saved_abef);
        state_cdgh = _mm_add_epi32(state_cdgh, saved_cdgh);

        shuffled = _mm_shuffle_epi32(state_abef, 0x1b);
        upper = _mm_shuffle_epi32(state_cdgh, 0xb1);
        shuffled = _mm_blend_epi16(shuffled, upper, 0xf0);
        upper = _mm_alignr_epi8(upper, shuffled, 8);

        _mm_storeu_si128(reinterpret_cast<__m128i*>(state), shuffled);
        _mm_storeu_si128(
            reinterpret_cast<__m128i*>(state + 4), upper);
    }
#endif
}

}  // namespace

extern "C" void hf_sha256_init(hf_sha256_ctx* ctx) {
    if (ctx == nullptr) {
        return;
    }
    for (int index = 0; index < 8; ++index) {
        ctx->state[index] = INITIAL_STATE[index];
    }
    ctx->bit_count = 0;
    ctx->buffer_len = 0;
}

extern "C" void hf_sha256_update(hf_sha256_ctx* ctx, const uint8_t* data, size_t len) {
    if (ctx == nullptr || (data == nullptr && len != 0)) {
        return;
    }
    ctx->bit_count += static_cast<uint64_t>(len) * 8u;

    size_t offset = 0;
    if (ctx->buffer_len != 0) {
        const size_t take = len < (64 - ctx->buffer_len) ? len : (64 - ctx->buffer_len);
        for (size_t index = 0; index < take; ++index) {
            ctx->buffer[ctx->buffer_len + index] = data[index];
        }
        ctx->buffer_len += take;
        offset = take;
        if (ctx->buffer_len == 64) {
            process_block(ctx->state, ctx->buffer, 1);
            ctx->buffer_len = 0;
        }
    }
    if (offset < len) {
        const size_t remaining = len - offset;
        const size_t blocks = remaining / 64;
        if (blocks != 0) {
            process_block(ctx->state, data + offset, blocks);
            offset += blocks * 64;
        }
        if (offset < len) {
            const size_t leftover = len - offset;
            for (size_t index = 0; index < leftover; ++index) {
                ctx->buffer[index] = data[offset + index];
            }
            ctx->buffer_len = leftover;
        }
    }
}

extern "C" void hf_sha256_final(hf_sha256_ctx* ctx, uint8_t out[32]) {
    if (ctx == nullptr || out == nullptr) {
        return;
    }
    const uint64_t bit_count = ctx->bit_count;

    ctx->buffer[ctx->buffer_len++] = 0x80u;
    if (ctx->buffer_len > 56) {
        for (size_t index = ctx->buffer_len; index < 64; ++index) {
            ctx->buffer[index] = 0u;
        }
        process_block(ctx->state, ctx->buffer, 1);
        ctx->buffer_len = 0;
    }
    for (size_t index = ctx->buffer_len; index < 56; ++index) {
        ctx->buffer[index] = 0u;
    }
    for (int index = 0; index < 8; ++index) {
        ctx->buffer[56 + index] = static_cast<uint8_t>(
            (bit_count >> (56 - index * 8)) & 0xffu);
    }
    process_block(ctx->state, ctx->buffer, 1);

    for (int index = 0; index < 8; ++index) {
        write_be32(out + index * 4, ctx->state[index]);
    }
}

extern "C" void hf_sha256(const uint8_t* data, size_t len, uint8_t out[32]) {
    if (out == nullptr) {
        return;
    }
    hf_sha256_ctx ctx;
    hf_sha256_init(&ctx);
    if (data != nullptr && len != 0) {
        hf_sha256_update(&ctx, data, len);
    }
    hf_sha256_final(&ctx, out);
}
