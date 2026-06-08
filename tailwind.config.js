/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // ── macOS System Colors ──
        macos: {
          blue:    { DEFAULT: "#007AFF", light: "#5AC8FA", dark: "#0A84FF" },
          indigo:  { DEFAULT: "#5856D6", light: "#7D7AFF", dark: "#4E4CAD" },
          purple:  { DEFAULT: "#AF52DE", light: "#DA8FFF", dark: "#9B4DD4" },
          pink:    { DEFAULT: "#FF2D55", light: "#FF6B8A", dark: "#E62E4F" },
          red:     { DEFAULT: "#FF3B30", light: "#FF6B5E", dark: "#E62E24" },
          orange:  { DEFAULT: "#FF9500", light: "#FFAD42", dark: "#E68500" },
          yellow:  { DEFAULT: "#FFCC00", light: "#FFE066", dark: "#E6B800" },
          green:   { DEFAULT: "#34C759", light: "#6EE89A", dark: "#2DB04A" },
          teal:    { DEFAULT: "#5AC8FA", light: "#86D6FF", dark: "#4EB8F5" },
          cyan:    { DEFAULT: "#32D4F4", light: "#72DFFF", dark: "#2DC0E0" },
        },
        surface: {
          DEFAULT: "rgba(255, 255, 255, 0.85)",
          dark: "rgba(30, 30, 30, 0.85)",
        },
        primary: {
          50:  "#E8F4FF",
          100: "#D1E9FF",
          200: "#A3D3FF",
          300: "#6DBDFF",
          400: "#36A7FF",
          500: "#007AFF",
          600: "#0071E3",
          700: "#0058B3",
          800: "#004082",
          900: "#002851",
        },
      },
      backdropBlur: {
        xs: "2px",
        sm: "4px",
        md: "8px",
        lg: "20px",
        xl: "28px",
      },
      borderRadius: {
        "2xs": "4px",
        xs:  "6px",
        sm:  "8px",
        md:  "10px",
        lg:  "14px",
        xl:  "20px",
      },
      animation: {
        "fade-in":       "fadeIn 200ms cubic-bezier(0.16, 1, 0.3, 1)",
        "fade-out":      "fadeOut 150ms ease-in",
        "slide-up":      "slideUp 200ms cubic-bezier(0.16, 1, 0.3, 1)",
        "slide-down":    "slideDown 200ms cubic-bezier(0.16, 1, 0.3, 1)",
        "scale-in":      "scaleIn 200ms cubic-bezier(0.32, 0.72, 0, 1)",
        "spin":          "spin 1s linear infinite",
        "pulse-slow":    "pulse 2.5s cubic-bezier(0.4, 0, 0.6, 1) infinite",
        "pulse-glow":    "pulseGlow 1.5s ease-in-out infinite",
        "spin-around":   "spinAround 0.8s linear infinite",
      },
      keyframes: {
        fadeIn: {
          "0%":   { opacity: "0", transform: "scale(0.96)" },
          "100%": { opacity: "1", transform: "scale(1)" },
        },
        fadeOut: {
          "0%": { opacity: "1" },
          "100%": { opacity: "0" },
        },
        slideUp: {
          "0%":   { opacity: "0", transform: "translateY(8px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        slideDown: {
          "0%":   { opacity: "0", transform: "translateY(-8px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
        scaleIn: {
          "0%":   { opacity: "0", transform: "scale(0.92)" },
          "100%": { opacity: "1", transform: "scale(1)" },
        },
        pulseGlow: {
          "0%, 100%": { boxShadow: "0 0 0 0 rgba(0, 122, 255, 0.35)" },
          "50%":       { boxShadow: "0 0 0 8px rgba(0, 122, 255, 0)" },
        },
        spinAround: {
          from: { transform: "rotate(0deg)" },
          to:   { transform: "rotate(360deg)" },
        },
      },
      boxShadow: {
        "popup":          "0 8px 32px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.08)",
        "popup-dark":     "0 8px 32px rgba(0, 0, 0, 0.50), 0 2px 8px rgba(0, 0, 0, 0.35)",
        "card":           "0 2px 8px rgba(0, 0, 0, 0.07), 0 0 1px rgba(0, 0, 0, 0.04)",
        "card-dark":      "0 2px 8px rgba(0, 0, 0, 0.30), 0 0 1px rgba(0, 0, 0, 0.20)",
        "macos":         "0 4px 16px rgba(0, 0, 0, 0.10), 0 1px 4px rgba(0, 0, 0, 0.06)",
        "macos-lg":      "0 8px 40px rgba(0, 0, 0, 0.15), 0 4px 12px rgba(0, 0, 0, 0.08)",
        "glow-blue":     "0 0 20px rgba(0, 122, 255, 0.30)",
        "glow-purple":   "0 0 20px rgba(175, 82, 222, 0.30)",
      },
      fontFamily: {
        ui:      ["SF Pro Text", "Segoe UI", "-apple-system", "BlinkMacSystemFont", "system-ui", "sans-serif"],
        display: ["SF Pro Display", "Segoe UI", "-apple-system", "BlinkMacSystemFont", "system-ui", "sans-serif"],
      },
      transitionTimingFunction: {
        "expo-out":  "cubic-bezier(0.16, 1, 0.3, 1)",
        "spring":    "cubic-bezier(0.32, 0.72, 0, 1)",
        "smooth":    "cubic-bezier(0.4, 0, 0.2, 1)",
      },
    },
  },
  plugins: [],
};
