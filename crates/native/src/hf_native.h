#ifndef HF_NATIVE_H
#define HF_NATIVE_H

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#define HF_EXTERN extern "C" __declspec(dllexport)
#else
#define HF_EXTERN extern "C" __attribute__((visibility("default")))
#endif

#if defined(__cplusplus)
extern "C" {
#endif

/* Runtime CPU feature probes (1 = available, 0 = not). */
HF_EXTERN int hf_has_aes_ni(void);
HF_EXTERN int hf_has_sha_ni(void);

/*
 * AES-128-CBC raw block decryption (no padding removal) with AES-NI.
 * `input_len` must be a non-zero multiple of 16.
 * Returns 0 on success, -1 on invalid input, -2 when AES-NI is unavailable.
 */
HF_EXTERN int hf_aes128_cbc_decrypt(const uint8_t* key,
                                    const uint8_t* iv,
                                    const uint8_t* input,
                                    size_t input_len,
                                    uint8_t* output);

/* SHA-256 streaming state. 128 bytes of well-aligned storage. */
typedef struct hf_sha256_ctx {
    uint32_t state[8];
    uint64_t bit_count;
    size_t buffer_len;
    uint8_t buffer[64];
} hf_sha256_ctx;

HF_EXTERN void hf_sha256_init(hf_sha256_ctx* ctx);
HF_EXTERN void hf_sha256_update(hf_sha256_ctx* ctx, const uint8_t* data, size_t len);
HF_EXTERN void hf_sha256_final(hf_sha256_ctx* ctx, uint8_t out[32]);
HF_EXTERN void hf_sha256(const uint8_t* data, size_t len, uint8_t out[32]);

#if defined(__cplusplus)
}
#endif

#endif /* HF_NATIVE_H */
