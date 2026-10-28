/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: {
          950: "#080b12",
          900: "#0b0f18",
          850: "#0f1520",
          800: "#131a27",
          700: "#1b2435",
          600: "#263247",
          500: "#38465e",
        },
        accent: {
          300: "#7cf5c8",
          400: "#3ee6a4",
          500: "#16c98a",
          600: "#0da876",
        },
      },
      fontFamily: {
        sans: [
          "Inter",
          "Segoe UI",
          "system-ui",
          "-apple-system",
          "sans-serif",
        ],
        mono: ["Cascadia Code", "JetBrains Mono", "Consolas", "monospace"],
      },
      boxShadow: {
        glow: "0 0 0 1px rgba(62,230,164,.25), 0 12px 40px -12px rgba(22,201,138,.45)",
      },
      animation: {
        "fade-in": "fadeIn .16s ease-out",
        "slide-up": "slideUp .2s ease-out",
        shimmer: "shimmer 1.6s linear infinite",
      },
      keyframes: {
        fadeIn: {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        slideUp: {
          "0%": { opacity: "0", transform: "translateY(10px) scale(.99)" },
          "100%": { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        shimmer: {
          "0%": { backgroundPosition: "200% 0" },
          "100%": { backgroundPosition: "-200% 0" },
        },
      },
    },
  },
  plugins: [],
};
