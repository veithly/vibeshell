// A CSS variable containing a complete color is not parseable by Tailwind's
// default alpha modifier. Keep the full theme token for ordinary utilities
// and emit a real translucent color for /10, /30, etc. (including custom themes).
const themeColor = (name) => ({ opacityValue, opacityVariable } = {}) => {
  const color = `var(--${name})`;
  if (opacityValue === undefined || opacityVariable) return color;
  const alpha = Number(opacityValue);
  const weight = Number.isFinite(alpha) ? `${alpha * 100}%` : `calc(${opacityValue} * 100%)`;
  return `color-mix(in srgb, ${color} ${weight}, transparent)`;
};

/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        // Tokyo Night theme colors - using CSS variables for dynamic theming
        'tokyo-bg': themeColor('tokyo-bg'),
        'tokyo-bg-dark': themeColor('tokyo-bg-dark'),
        'tokyo-bg-hl': themeColor('tokyo-bg-hl'),
        'tokyo-fg': themeColor('tokyo-fg'),
        'tokyo-fg-dark': themeColor('tokyo-fg-dark'),
        'tokyo-selection': themeColor('tokyo-selection'),
        'tokyo-comment': themeColor('tokyo-comment'),
        'tokyo-red': themeColor('tokyo-red'),
        'tokyo-green': themeColor('tokyo-green'),
        'tokyo-yellow': themeColor('tokyo-yellow'),
        'tokyo-blue': themeColor('tokyo-blue'),
        'tokyo-on-accent': themeColor('tokyo-on-accent'),
        'tokyo-magenta': themeColor('tokyo-magenta'),
        'tokyo-cyan': themeColor('tokyo-cyan'),
        'tokyo-orange': themeColor('tokyo-orange'),
      },
      animation: {
        'fade-in': 'fadeIn 0.15s ease-out forwards',
        'slide-in-right': 'slideInRight 0.3s ease-out',
        'in': 'animateIn 0.15s ease-out',
        'zoom-in-95': 'zoomIn95 0.15s ease-out',
        'slide-in-from-bottom-2': 'slideInFromBottom 0.15s ease-out',
      },
      keyframes: {
        fadeIn: {
          '0%': { opacity: '0', transform: 'scale(0.95)' },
          '100%': { opacity: '1', transform: 'scale(1)' },
        },
        slideInRight: {
          '0%': { opacity: '0', transform: 'translateX(1rem)' },
          '100%': { opacity: '1', transform: 'translateX(0)' },
        },
        animateIn: {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        zoomIn95: {
          '0%': { transform: 'scale(0.95)' },
          '100%': { transform: 'scale(1)' },
        },
        slideInFromBottom: {
          '0%': { transform: 'translateY(0.5rem)' },
          '100%': { transform: 'translateY(0)' },
        },
      },
    },
  },
  plugins: [],
}
