import type { AvatarState } from "../lib/types";

interface AvatarProps {
  state: AvatarState;
  size?: number;
  className?: string;
}

export function Avatar({ state, size = 64, className }: AvatarProps) {
  const eyeY = state === "thinking" ? 24.5 : 27.2;
  const looking = state === "thinking" ? -1.6 : state === "listening" ? 1.2 : 0;
  const blink = state === "warming";
  const closed = state === "error";

  return (
    <svg
      className={`avatar-mark avatar-mark--${state}${className ? ` ${className}` : ""}`}
      width={size}
      height={size}
      viewBox="0 0 72 72"
      role="img"
      aria-label={`Bernard is ${state}`}
    >
      <defs>
        <radialGradient id="bernard-face" cx="38%" cy="30%" r="78%">
          <stop offset="0%" stopColor="#fbf2e1" />
          <stop offset="72%" stopColor="#ecdcbe" />
          <stop offset="100%" stopColor="#dcc7a4" />
        </radialGradient>
        <linearGradient id="bernard-body" x1="12" y1="8" x2="62" y2="66">
          <stop offset="0%" stopColor="#292a2f" />
          <stop offset="100%" stopColor="#141519" />
        </linearGradient>
        <linearGradient id="bernard-ring" x1="8" y1="8" x2="64" y2="64">
          <stop offset="0%" stopColor="#f2e6cf" stopOpacity="0.55" />
          <stop offset="55%" stopColor="#c99a62" stopOpacity="0.4" />
          <stop offset="100%" stopColor="#f2e6cf" stopOpacity="0.12" />
        </linearGradient>
        <clipPath id="bernard-clip">
          <circle cx="36" cy="36" r="30" />
        </clipPath>
        <filter id="bernard-soft" x="-30%" y="-30%" width="160%" height="160%">
          <feGaussianBlur stdDeviation="2.4" />
        </filter>
      </defs>

      <circle
        className="avatar-glow"
        cx="36"
        cy="37"
        r="27"
        fill="var(--avatar-glow, #c99a62)"
        opacity="0.22"
        filter="url(#bernard-soft)"
      />

      <g clipPath="url(#bernard-clip)">
        <circle cx="36" cy="36" r="30" fill="url(#bernard-body)" />
        <circle cx="36" cy="36" r="30" fill="none" stroke="url(#bernard-ring)" strokeWidth="1.6" />
      </g>

      <g className="avatar-orbit" opacity="0">
        <circle cx="36" cy="4.5" r="2.1" fill="#d9a15f" />
        <circle cx="62" cy="48" r="1.5" fill="#d9a15f" opacity="0.7" />
        <circle cx="10" cy="48" r="1.5" fill="#d9a15f" opacity="0.5" />
      </g>

      <circle cx="36" cy="36.5" r="23.4" fill="url(#bernard-face)" />
      <ellipse cx="36" cy="31" rx="18" ry="12" fill="#fff8e9" opacity="0.28" />

      {closed ? (
        <g stroke="#212226" strokeWidth="2" strokeLinecap="round">
          <path d="m27.4 28.4 4.2 4.2M31.6 28.4l-4.2 4.2" />
          <path d="m40.4 28.4 4.2 4.2M44.6 28.4l-4.2 4.2" />
        </g>
      ) : (
        <g
          className={`avatar-eyes${blink ? " avatar-eyes--closed" : ""}`}
          fill="#202126"
          transform={`translate(${looking} 0)`}
        >
          <ellipse cx="28.3" cy={eyeY} rx="2.5" ry={state === "thinking" ? 3.3 : 3.7} />
          <ellipse cx="43.7" cy={eyeY} rx="2.5" ry={state === "thinking" ? 3.3 : 3.7} />
          <circle cx="29.1" cy={eyeY - 1.2} r="0.75" fill="#fff7e6" opacity="0.8" />
          <circle cx="44.5" cy={eyeY - 1.2} r="0.75" fill="#fff7e6" opacity="0.8" />
        </g>
      )}

      <path
        className="avatar-brow avatar-brow--left"
        d="M24.2 21.8c1.4-1.2 3.4-1.6 5.3-1"
        fill="none"
        stroke="#232428"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity="0.55"
        transform={`rotate(${state === "thinking" ? -7 : 0} 27 21)`}
      />
      <path
        className="avatar-brow avatar-brow--right"
        d="M42.5 20.8c1.9-.6 3.9-.2 5.3 1"
        fill="none"
        stroke="#232428"
        strokeWidth="1.5"
        strokeLinecap="round"
        opacity="0.55"
        transform={`rotate(${state === "thinking" ? 7 : 0} 45 21)`}
      />

      {state === "answering" ? (
        <g className="avatar-mouth avatar-mouth--talking">
          <ellipse cx="36" cy="41.4" rx="4.6" ry="3.4" fill="#2a2220" />
          <ellipse cx="36" cy="42.7" rx="2.6" ry="1.5" fill="#c97a64" opacity="0.8" />
        </g>
      ) : state === "thinking" ? (
        <path
          className="avatar-mouth"
          d="M32.4 41.6h7.2"
          fill="none"
          stroke="#2a2220"
          strokeWidth="1.9"
          strokeLinecap="round"
          opacity="0.8"
        />
      ) : state === "error" ? (
        <path
          className="avatar-mouth"
          d="M31.7 43.4c1.3-1.5 2.7-2.2 4.3-2.2s3 .7 4.3 2.2"
          fill="none"
          stroke="#2a2220"
          strokeWidth="1.9"
          strokeLinecap="round"
        />
      ) : (
        <path
          className="avatar-mouth"
          d="M30.9 39.9c1.5 2 3.2 3 5.1 3s3.6-1 5.1-3"
          fill="none"
          stroke="#2a2220"
          strokeWidth="1.9"
          strokeLinecap="round"
        />
      )}

      <g className="avatar-bars" opacity="0">
        <rect x="27" y="49.5" width="2.2" height="6" rx="1.1" fill="#d9a15f" />
        <rect x="32" y="47.5" width="2.2" height="10" rx="1.1" fill="#d9a15f" />
        <rect x="37" y="48.6" width="2.2" height="8" rx="1.1" fill="#d9a15f" />
        <rect x="42" y="50" width="2.2" height="5.2" rx="1.1" fill="#d9a15f" />
      </g>

      <circle
        className="avatar-status"
        cx="59.5"
        cy="59.5"
        r="5.4"
        fill="var(--avatar-status, #6f8a6d)"
        stroke="#17181b"
        strokeWidth="2.4"
      />
    </svg>
  );
}
