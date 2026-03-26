import { PortalClient } from "../../components/portal-client";

export default function PlatformStaffPortalPage() {
  return (
    <PortalClient
      expectedSubjectType="PLATFORM_STAFF"
      portalApiPath="/api/portal/platform/home"
      title="Platform Staff Portal"
      description="这是 Platform Staff 对应的 portal 页面，当前展示主体信息、Platform portal API 返回数据和会话列表。"
    />
  );
}
