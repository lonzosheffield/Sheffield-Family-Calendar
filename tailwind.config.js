/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html,css}"],
  theme: {
    extend: {
      colors: {
        "sheffield-light": "#8BB5DA",
        "sheffield-dark": "#2672B3",
        "sheffield-accent": "#E86A58",
        "sheffield-sun": "#F4D03F",
        "sheffield-paper": "#FDFDFD",
      },
      fontFamily: {
        display: ["Nunito", "ui-sans-serif", "system-ui", "sans-serif"],
      },
    },
  },
  plugins: [],
};
