// Persistente Seitenleiste: von jedem Bereich direkt in jeden anderen, ohne
// Zurück-Button. Optionale Bereiche (Ansagen, Monitore) bleiben immer sichtbar,
// sind aber ausgegraut, solange sie in den Einstellungen nicht aktiviert sind –
// ein Klick darauf springt in die Einstellungen zum passenden Abschnitt.
import {
  Activity,
  LayoutGrid,
  type LucideIcon,
  Megaphone,
  SlidersHorizontal,
  Tablet,
  Trophy,
  Tv,
  Wrench,
} from "lucide-react";
import type { AppConfig } from "../types";

/** Bereiche, die über die Seitenleiste erreichbar sind. */
export type NavView =
  | "dashboard"
  | "fields"
  | "tablets"
  | "announce"
  | "monitors"
  | "winners"
  | "settings"
  | "maintenance";

/** Abschnitts-Anker in den Einstellungen (für den Sprung aus einem
 *  ausgegrauten Menüpunkt). */
export type SettingsFocus = "ansagen" | "court-monitor";

interface NavItem {
  view: NavView;
  label: string;
  icon: LucideIcon;
  /** false → ausgegraut; Klick führt in die Einstellungen. */
  enabled: boolean;
  /** Einstellungen-Abschnitt, zu dem ein ausgegrauter Punkt springt. */
  focus?: SettingsFocus;
}

function items(config: AppConfig): NavItem[] {
  return [
    { view: "dashboard", label: "Status", icon: Activity, enabled: true },
    { view: "fields", label: "Spielübersicht", icon: LayoutGrid, enabled: true },
    { view: "tablets", label: "Tablets", icon: Tablet, enabled: true },
    {
      view: "announce",
      label: "Ansagen",
      icon: Megaphone,
      enabled: config.announce.enabled,
      focus: "ansagen",
    },
    {
      view: "monitors",
      label: "Monitore",
      icon: Tv,
      enabled: config.court_monitor.enabled,
      focus: "court-monitor",
    },
    {
      view: "winners",
      label: "Siegerehrung",
      icon: Trophy,
      enabled: config.court_monitor.enabled,
      focus: "court-monitor",
    },
    {
      view: "settings",
      label: "Einstellungen",
      icon: SlidersHorizontal,
      enabled: true,
    },
    {
      view: "maintenance",
      label: "Wartung",
      icon: Wrench,
      enabled: true,
    },
  ];
}

export function SideNav({
  current,
  config,
  onNavigate,
}: {
  current: NavView;
  config: AppConfig;
  onNavigate: (view: NavView, focus?: SettingsFocus) => void;
}) {
  return (
    <nav className="flex w-44 shrink-0 flex-col gap-1 border-r border-slate-200 bg-white p-2">
      {items(config).map((item) => {
        const Icon = item.icon;
        const active = current === item.view;
        // Ausgegrauter (noch nicht aktivierter) Punkt: gedämpft, Klick führt
        // zum passenden Einstellungen-Abschnitt statt auf die leere Seite.
        const handleClick = () =>
          item.enabled
            ? onNavigate(item.view)
            : onNavigate("settings", item.focus);
        return (
          <button
            key={item.view}
            onClick={handleClick}
            title={
              item.enabled
                ? item.label
                : `${item.label} – in den Einstellungen aktivieren`
            }
            aria-current={active ? "page" : undefined}
            className={`flex items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm
                        font-medium transition-colors ${
                          active
                            ? "bg-slate-800 text-white"
                            : item.enabled
                              ? "text-slate-700 hover:bg-slate-100"
                              : "text-slate-400 hover:bg-slate-50"
                        }`}
          >
            <Icon size={17} strokeWidth={2} className="shrink-0" />
            <span className="min-w-0 flex-1 truncate">{item.label}</span>
            {!item.enabled && (
              <span
                className="shrink-0 rounded bg-slate-100 px-1.5 py-0.5 text-[10px]
                           font-semibold uppercase tracking-wide text-slate-400"
              >
                aus
              </span>
            )}
          </button>
        );
      })}
    </nav>
  );
}
