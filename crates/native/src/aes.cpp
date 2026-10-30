// AES-128-CBC decryption accelerated with the AES-NI instruction set.
//
// HyperFetch downloads unencrypted media and standard AES-128 HLS segments
// whose key URI is published in the playlist. DRM systems remain out of scope.

#include "hf_native.h"

#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
#define HF_X86 1
#if defined(_MSC_VER)
#include <intrin.h>
#else
#include <cpuid.h>
#endif
#include <wmmintrin.h>
#endif

namespace {

#if defined(HF_X86)
bool cpu_bit(int reg_index, unsigned int bit, bool needs_subleaf = false) {
#if defined(_MSC_VER)
    int regs[4];
    if (needs_subleaf) {
        __cpuidex(regs, 1, 0);
    } else {
        __cpuid(regs, 1);
    }
    return ((static_cast<unsigned int>(regs[reg_index]) >> bit) & 1u) != 0u;
#else
    unsigned int eax = 0, ebx = 0, ecx = 0, edx = 0;
    if (!__get_cpuid(1, &eax, &ebx, &ecx, &edx)) {
        return false;
    }
    unsigned int values[4] = {eax, ebx, ecx, edx};
    return ((values[reg_index] >> bit) & 1u) != 0u;
#endif
}
#endif

#define HF_KEY_EXPAND(previous, rcon)                                   \
    do {                                                               \
        __m128i generated = _mm_aeskeygenassist_si128(previous, rcon);  \
        generated = _mm_shuffle_epi32(generated, 0xff);                \
        __m128i mixed = previous;                                      \
        mixed = _mm_xor_si128(mixed, _mm_slli_si128(mixed, 4));        \
        mixed = _mm_xor_si128(mixed, _mm_slli_si128(mixed, 4));        \
        mixed = _mm_xor_si128(mixed, _mm_slli_si128(mixed, 4));        \
        result = _mm_xor_si128(mixed, generated);                      \
    } while (0)

void expand_key(const uint8_t* key, __m128i schedule[11]) {
    __m128i result = _mm_loadu_si128(reinterpret_cast<const __m128i*>(key));
    schedule[0] = result;
    HF_KEY_EXPAND(schedule[0], 0x01);
    schedule[1] = result;
    HF_KEY_EXPAND(schedule[1], 0x02);
    schedule[2] = result;
    HF_KEY_EXPAND(schedule[2], 0x04);
    schedule[3] = result;
    HF_KEY_EXPAND(schedule[3], 0x08);
    schedule[4] = result;
    HF_KEY_EXPAND(schedule[4], 0x10);
    schedule[5] = result;
    HF_KEY_EXPAND(schedule[5], 0x20);
    schedule[6] = result;
    HF_KEY_EXPAND(schedule[6], 0x40);
    schedule[7] = result;
    HF_KEY_EXPAND(schedule[7], 0x80);
    schedule[8] = result;
    HF_KEY_EXPAND(schedule[8], 0x01b);
    schedule[9] = result;
    HF_KEY_EXPAND(schedule[9], 0x36);
    schedule[10] = result;
}

}  // namespace

extern "C" int hf_has_aes_ni(void) {
#if defined(HF_X86)
    return cpu_bit(2, 25) ? 1 : 0;
#else
    return 0;
#endif
}

extern "C" int hf_has_sha_ni(void) {
#if defined(HF_X86) && defined(__SHA__)
    return cpu_bit(2, 29) ? 1 : 0;
#else
    return 0;
#endif
}

extern "C" int hf_aes128_cbc_decrypt(const uint8_t* key,
                                     const uint8_t* iv,
                                     const uint8_t* input,
                                     size_t input_len,
                                     uint8_t* output) {
    if (input_len == 0 || (input_len & 15u) != 0u || key == nullptr ||
        iv == nullptr || input == nullptr || output == nullptr) {
        return -1;
    }
#if !defined(HF_X86)
    (void)key;
    (void)iv;
    (void)input;
    (void)output;
    return -2;
#else
    if (!cpu_bit(2, 25)) {
        return -2;
    }

    __m128i schedule[11];
    expand_key(key, schedule);

    __m128i chain = _mm_loadu_si128(reinterpret_cast<const __m128i*>(iv));
    for (size_t offset = 0; offset < input_len; offset += 16) {
        const __m128i cipher =
            _mm_loadu_si128(reinterpret_cast<const __m128i*>(input + offset));
        const __m128i stored = cipher;

        __m128i state = _mm_xor_si128(cipher, schedule[10]);
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[9]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[8]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[7]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[6]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[5]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[4]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[3]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[2]));
        state = _mm_aesdec_si128(state, _mm_aesimc_si128(schedule[1]));
        state = _mm_aesdeclast_si128(state, schedule[0]);
        state = _mm_xor_si128(state, chain);

        _mm_storeu_si128(reinterpret_cast<__m128i*>(output + offset), state);
        chain = stored;
    }
    return 0;
#endif
}
