import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DictationStats {
  total_entries: number;
  total_words: number;
  today_entries: number;
  avg_words: number;
  minutes_saved: number;
}

export default function Stats() {
  const [stats, setStats] = useState<DictationStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback((): void => {
    invoke<DictationStats>("dictation_stats")
      .then((s) => {
        setStats(s);
        setError(null);
      })
      .catch(() => setError("Statistiques indisponibles (backend en cours)."));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  if (error) return <p className="field-hint">{error}</p>;
  if (!stats) return <p className="muted">Chargement des statistiques…</p>;

  const cards: Array<{ label: string; value: string }> = [
    { label: "Mots dictés", value: String(stats.total_words) },
    { label: "Temps gagné", value: `${Math.round(stats.minutes_saved)} min` },
    { label: "Dictées aujourd'hui", value: String(stats.today_entries) },
    { label: "Mots / dictée", value: stats.avg_words.toFixed(1) },
  ];

  return (
    <div className="stats-grid" role="group" aria-label="Statistiques de dictée">
      {cards.map((c) => (
        <div key={c.label} className="stat-card">
          <span className="stat-value">{c.value}</span>
          <span className="stat-label">{c.label}</span>
        </div>
      ))}
      <div className="row stats-foot">
        <span className="field-hint">{stats.total_entries} dictées au total (150 mots/min).</span>
        <button type="button" className="ghost" onClick={refresh}>
          Actualiser
        </button>
      </div>
    </div>
  );
}
