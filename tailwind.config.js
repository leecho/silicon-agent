/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        background: "var(--background)",
        foreground: "var(--foreground)",
        "foreground-secondary": "var(--foreground-secondary)",
        "foreground-muted": "var(--foreground-muted)",
        surface: "var(--surface)",
        card: "var(--card)",
        "card-foreground": "var(--card-foreground)",
        popover: "var(--popover)",
        "popover-foreground": "var(--popover-foreground)",
        primary: "rgb(var(--primary-rgb) / <alpha-value>)",
        "primary-foreground": "var(--primary-foreground)",
        secondary: "var(--secondary)",
        "secondary-foreground": "var(--secondary-foreground)",
        muted: "var(--muted)",
        "muted-foreground": "var(--muted-foreground)",
        accent: "var(--accent)",
        "accent-foreground": "var(--accent-foreground)",
        success: "rgb(var(--success-rgb) / <alpha-value>)",
        "success-foreground": "var(--success-foreground)",
        "success-subtle": "var(--success-subtle)",
        "success-border": "var(--success-border)",
        warning: "rgb(var(--warning-rgb) / <alpha-value>)",
        "warning-foreground": "var(--warning-foreground)",
        "warning-subtle": "var(--warning-subtle)",
        "warning-border": "var(--warning-border)",
        danger: "rgb(var(--danger-rgb) / <alpha-value>)",
        "danger-foreground": "var(--danger-foreground)",
        "danger-subtle": "var(--danger-subtle)",
        "danger-border": "var(--danger-border)",
        destructive: "rgb(var(--destructive-rgb) / <alpha-value>)",
        "destructive-foreground": "var(--destructive-foreground)",
        "border-subtle": "var(--border-subtle)",
        border: "var(--border)",
        "border-strong": "var(--border-strong)",
        input: "var(--input)",
        ring: "rgb(var(--ring-rgb) / <alpha-value>)"
      },
      borderRadius: {
        sm: "calc(var(--radius) - 2px)",
        md: "var(--radius)",
        lg: "calc(var(--radius) + 2px)",
        xl: "calc(var(--radius) + 4px)"
      }
    }
  },
  plugins: []
};
