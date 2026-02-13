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
        'tokyo-bg': 'var(--tokyo-bg)',
        'tokyo-bg-dark': 'var(--tokyo-bg-dark)',
        'tokyo-bg-hl': 'var(--tokyo-bg-hl)',
        'tokyo-fg': 'var(--tokyo-fg)',
        'tokyo-fg-dark': 'var(--tokyo-fg-dark)',
        'tokyo-selection': 'var(--tokyo-selection)',
        'tokyo-comment': 'var(--tokyo-comment)',
        'tokyo-red': 'var(--tokyo-red)',
        'tokyo-green': 'var(--tokyo-green)',
        'tokyo-yellow': 'var(--tokyo-yellow)',
        'tokyo-blue': 'var(--tokyo-blue)',
        'tokyo-magenta': 'var(--tokyo-magenta)',
        'tokyo-cyan': 'var(--tokyo-cyan)',
        'tokyo-orange': 'var(--tokyo-orange)',
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
