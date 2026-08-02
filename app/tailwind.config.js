/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Derived from logo.png's gradient mark.
        aeblue: {
          400: "#7c9bff",
          500: "#5b7cfa",
          600: "#4661e0",
        },
        aepurple: {
          400: "#c08bff",
          500: "#a866f0",
          600: "#8c4fd6",
        },
        aecyan: {
          400: "#4fd8f5",
          500: "#22c3e6",
        },
        ink: {
          950: "#0a0a0d",
          900: "#121216",
          850: "#17171d",
          800: "#1e1e26",
          700: "#2a2a34",
          600: "#3a3a46",
        },
      },
      backgroundImage: {
        "aetheria-gradient":
          "linear-gradient(135deg, #5b7cfa 0%, #a866f0 100%)",
      },
    },
  },
  plugins: [],
};
