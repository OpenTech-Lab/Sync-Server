"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { LucideIcon } from "lucide-react";
import {
  Activity,
  Bot,
  LayoutDashboard,
  Newspaper,
  ScrollText,
  ShieldAlert,
  ShieldCheck,
  SlidersHorizontal,
  Sticker,
  Users,
} from "lucide-react";

import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";

export type AdminNavIcon =
  | "overview"
  | "guild"
  | "users"
  | "moderation"
  | "stickers"
  | "config"
  | "news"
  | "agents"
  | "audit";

export type AdminNavSection = "workspace" | "operations" | "system";

export type AdminNavItem = {
  href: string;
  label: string;
  icon: AdminNavIcon;
  section: AdminNavSection;
};

const SECTION_LABELS: Record<AdminNavSection, string> = {
  workspace: "Workspace",
  operations: "Operations",
  system: "System",
};

const ICONS: Record<AdminNavIcon, LucideIcon> = {
  overview: LayoutDashboard,
  guild: ShieldCheck,
  users: Users,
  moderation: ShieldAlert,
  stickers: Sticker,
  config: SlidersHorizontal,
  news: Newspaper,
  agents: Bot,
  audit: ScrollText,
};

function normalizedPath(pathname: string) {
  const withoutBasePath = pathname.replace(/^\/admin(?=\/|$)/, "");
  return withoutBasePath || "/";
}

function isItemActive(pathname: string, href: string) {
  const currentPath = normalizedPath(pathname);
  return currentPath === href || currentPath.startsWith(`${href}/`);
}

export function AdminNav({ items }: { items: AdminNavItem[] }) {
  const pathname = usePathname();
  const { isMobile, setOpenMobile } = useSidebar();

  function closeMobileSidebar() {
    if (isMobile) {
      setOpenMobile(false);
    }
  }

  return (
    <nav aria-label="Admin navigation">
      {(["workspace", "operations", "system"] as const).map((section) => {
        const sectionItems = items.filter((item) => item.section === section);
        if (sectionItems.length === 0) {
          return null;
        }

        return (
          <SidebarGroup className="px-2 py-2" key={section}>
            <SidebarGroupLabel className="px-2 text-[10px] font-semibold uppercase tracking-[0.18em] text-sidebar-foreground/60">
              {SECTION_LABELS[section]}
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {sectionItems.map((item) => {
                  const active = isItemActive(pathname, item.href);
                  const Icon = ICONS[item.icon];

                  return (
                    <SidebarMenuItem key={item.href}>
                      <SidebarMenuButton
                        asChild
                        isActive={active}
                        tooltip={item.label}
                      >
                        <Link href={item.href} onClick={closeMobileSidebar}>
                          <Icon aria-hidden="true" />
                          <span>{item.label}</span>
                          {active ? (
                            <Activity aria-hidden="true" className="ml-auto size-3.5 opacity-70 group-data-[collapsible=icon]:hidden" />
                          ) : null}
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  );
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        );
      })}
    </nav>
  );
}
