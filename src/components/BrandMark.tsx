export function BrandMark({ compact = false }: { compact?: boolean }) {
  return (
    <div className="brand" aria-label="CodexTurnChime">
      <svg className="brand-mark" viewBox="0 0 52 28" role="img" aria-hidden="true">
        <path d="M3 15h5l3-8 5 15 5-19 5 22 5-16 4 9h7" />
        <circle cx="48" cy="15" r="3.5" />
      </svg>
      {!compact && (
        <div>
          <strong>CodexTurnChime</strong>
          <span>v0.1.0-beta.2</span>
        </div>
      )}
    </div>
  );
}
