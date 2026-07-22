import { Button, Skeleton } from "../../../components/ui";
import { SettingsSection, SettingItem, StatusBadge } from "../../../components/settings/SettingsControls";
import { useSystemPermissions } from "../../../hooks/useSystemPermissions";
import { PERMISSION_ROWS, systemPermissionsCopy as copy } from "./systemPermissions";
import type { PermissionRow } from "../../../api";
import { appRelaunch } from "../../../api";

export function SystemPermissionsSection() {
  const { rows, loadState, refresh, authorize } = useSystemPermissions();
  const byKind = new Map<string, PermissionRow>(rows.map((r) => [r.kind, r]));

  return (
    <SettingsSection title={copy.sectionTitle} description={copy.sectionDesc}>
      {loadState === "loading" && (
        <div className="px-5 py-4">
          <Skeleton lines={4} />
        </div>
      )}

      {loadState === "error" && (
        <SettingItem title={copy.sectionTitle} description="检测授权状态失败">
          <Button tone="outline" onClick={() => void refresh()}>
            {copy.recheck}
          </Button>
        </SettingItem>
      )}

      {loadState === "ready" &&
        PERMISSION_ROWS.map((cfg) => {
          const row = byKind.get(cfg.kind);
          const granted = row?.state === "granted";
          const canRequest = row?.canRequest ?? false;
          const needsRelaunch = row?.needsRelaunch ?? false;
          return (
            <SettingItem key={cfg.kind} title={cfg.title} description={cfg.description} icon={cfg.icon}>
              <div className="flex flex-col items-end gap-1">
                <div className="flex items-center gap-2">
                  {granted ? (
                    <StatusBadge label={copy.granted} tone="success" />
                  ) : (
                    <StatusBadge label={copy.denied} tone="warning" />
                  )}
                  {!granted && (
                    <Button tone="outline" onClick={() => void authorize(cfg.kind, canRequest)}>
                      {canRequest ? copy.authorize : copy.openSettings}
                    </Button>
                  )}
                  {!granted && needsRelaunch && (
                    <Button tone="outline" onClick={() => void appRelaunch()}>
                      {copy.relaunchNow}
                    </Button>
                  )}
                </div>
                {!granted && needsRelaunch && (
                  <span className="text-xs text-foreground-muted">{copy.relaunchHint}</span>
                )}
              </div>
            </SettingItem>
          );
        })}
    </SettingsSection>
  );
}
