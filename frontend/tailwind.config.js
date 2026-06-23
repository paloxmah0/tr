/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: { 950: "#0a0e14", 900: "#0d1117", 850: "#11161d", 800: "#161b22", 700: "#21262d", 600: "#2d333b" },
        accent: { DEFAULT: "#58a6ff", dim: "#1f6feb" },
        ok: "#3fb950", warn: "#d29922", bad: "#f85149", muted: "#8b949e",
      },
    },
  },
  plugins: [],
};
