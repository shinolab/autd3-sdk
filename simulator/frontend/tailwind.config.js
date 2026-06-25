/** @type {import('tailwindcss').Config} */
module.exports = {
  mode: "all",
  content: ["./src/**/*.{rs,html}", "./index.html"],
  theme: {
    extend: {},
  },
  plugins: [require("daisyui")],
};
