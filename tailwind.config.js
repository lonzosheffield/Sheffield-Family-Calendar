/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{rs,html,css}"],
  // D4.1 lands the `font-poster` family and its @font-face before D4.3
  // wires any component to the `font-poster` class (its own later task,
  // §4 "D4.4 lands before D4.3" / D4.1 owns none of `src/client/`). Without
  // a safelist entry the JIT scanner would tree-shake the class out of
  // `assets/tailwind.css` until D4.3 lands, which would fail this task's
  // own acceptance (c) ("compiled assets/tailwind.css contains `Baloo 2`
  // in a `.font-poster` rule").
  safelist: ["font-poster"],
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
        poster: [
          "'Baloo 2'",
          "Nunito",
          "ui-sans-serif",
          "system-ui",
          "sans-serif",
        ],
      },
    },
  },
  plugins: [],
};
