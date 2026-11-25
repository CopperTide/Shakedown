// Curated vendor discovery and planned adapter scope. Never inferred runtime availability.
export interface CatalogEntry {
  id: string; name: string; vendor: string; logo: string | null; category: string;
  industries: string[]; protocols: string[]; areas: string[]; docs: string; status: 'planned';
}
export const CATALOG: CatalogEntry[] = [
  {
    "id": "oracle-fusion",
    "name": "Oracle Fusion Cloud",
    "vendor": "Oracle",
    "logo": "oracle",
    "category": "ERP & finance",
    "industries": [
      "Manufacturing",
      "Financial services",
      "Retail"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Finance",
      "Procurement",
      "Supply chain"
    ],
    "docs": "https://docs.oracle.com/en/cloud/saas/",
    "status": "planned"
  },
  {
    "id": "oracle-netsuite",
    "name": "NetSuite",
    "vendor": "Oracle",
    "logo": "oracle",
    "category": "ERP & finance",
    "industries": [
      "Retail",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Customers",
      "Orders",
      "Accounting"
    ],
    "docs": "https://docs.oracle.com/en/cloud/saas/netsuite/",
    "status": "planned"
  },
  {
    "id": "sap-s4hana",
    "name": "SAP S/4HANA",
    "vendor": "SAP",
    "logo": "sap",
    "category": "ERP & finance",
    "industries": [
      "Manufacturing",
      "Logistics",
      "Retail"
    ],
    "protocols": [
      "REST API",
      "OData"
    ],
    "areas": [
      "Orders",
      "Inventory",
      "Procurement"
    ],
    "docs": "https://api.sap.com/",
    "status": "planned"
  },
  {
    "id": "dynamics365",
    "name": "Dynamics 365",
    "vendor": "Microsoft",
    "logo": null,
    "category": "ERP & finance",
    "industries": [
      "Manufacturing",
      "Retail",
      "Financial services"
    ],
    "protocols": [
      "REST API",
      "OData"
    ],
    "areas": [
      "Finance",
      "Sales",
      "Field service"
    ],
    "docs": "https://learn.microsoft.com/en-us/dynamics365/",
    "status": "planned"
  },
  {
    "id": "erpnext",
    "name": "ERPNext",
    "vendor": "Frappe",
    "logo": "erpnext",
    "category": "ERP & finance",
    "industries": [
      "Manufacturing",
      "Logistics",
      "Retail"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Documents",
      "Inventory",
      "Invoices"
    ],
    "docs": "https://docs.frappe.io/framework/user/en/api/rest",
    "status": "planned"
  },
  {
    "id": "salesforce",
    "name": "Salesforce",
    "vendor": "Salesforce",
    "logo": "salesforce",
    "category": "CRM & support",
    "industries": [
      "Financial services",
      "Retail",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Records",
      "Queries",
      "Metadata"
    ],
    "docs": "https://developer.salesforce.com/docs/atlas.en-us.api_rest.meta/api_res/intro_rest_resources.htm",
    "status": "planned"
  },
  {
    "id": "hubspot",
    "name": "HubSpot",
    "vendor": "HubSpot",
    "logo": "hubspot",
    "category": "CRM & support",
    "industries": [
      "Retail",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Contacts",
      "Companies",
      "Deals"
    ],
    "docs": "https://developers.hubspot.com/docs/api/overview",
    "status": "planned"
  },
  {
    "id": "zendesk",
    "name": "Zendesk",
    "vendor": "Zendesk",
    "logo": "zendesk",
    "category": "CRM & support",
    "industries": [
      "Retail",
      "Travel",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Tickets",
      "Users",
      "Organizations"
    ],
    "docs": "https://developer.zendesk.com/api-reference/",
    "status": "planned"
  },
  {
    "id": "servicenow",
    "name": "ServiceNow",
    "vendor": "ServiceNow",
    "logo": null,
    "category": "IT & engineering",
    "industries": [
      "Healthcare",
      "Financial services",
      "Manufacturing"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Incidents",
      "Service requests",
      "Assets"
    ],
    "docs": "https://developer.servicenow.com/",
    "status": "planned"
  },
  {
    "id": "jira",
    "name": "Jira",
    "vendor": "Atlassian",
    "logo": "jira",
    "category": "IT & engineering",
    "industries": [
      "Technology",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Issues",
      "Projects",
      "Workflows"
    ],
    "docs": "https://developer.atlassian.com/cloud/jira/platform/rest/v3/intro/",
    "status": "planned"
  },
  {
    "id": "github",
    "name": "GitHub",
    "vendor": "GitHub",
    "logo": "github",
    "category": "IT & engineering",
    "industries": [
      "Technology",
      "Professional services"
    ],
    "protocols": [
      "REST API",
      "MCP"
    ],
    "areas": [
      "Repositories",
      "Issues",
      "Pull requests"
    ],
    "docs": "https://github.com/github/github-mcp-server",
    "status": "planned"
  },
  {
    "id": "slack",
    "name": "Slack",
    "vendor": "Salesforce",
    "logo": "slack",
    "category": "Collaboration",
    "industries": [
      "Technology",
      "Professional services"
    ],
    "protocols": [
      "REST API",
      "MCP"
    ],
    "areas": [
      "Channels",
      "Messages",
      "Search"
    ],
    "docs": "https://docs.slack.dev/",
    "status": "planned"
  },
  {
    "id": "microsoft365",
    "name": "Microsoft 365",
    "vendor": "Microsoft",
    "logo": null,
    "category": "Collaboration",
    "industries": [
      "Healthcare",
      "Financial services",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Mail",
      "Calendars",
      "Files"
    ],
    "docs": "https://learn.microsoft.com/en-us/graph/overview",
    "status": "planned"
  },
  {
    "id": "google-workspace",
    "name": "Google Workspace",
    "vendor": "Google",
    "logo": "google",
    "category": "Collaboration",
    "industries": [
      "Technology",
      "Education",
      "Professional services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Mail",
      "Documents",
      "Calendars"
    ],
    "docs": "https://developers.google.com/workspace",
    "status": "planned"
  },
  {
    "id": "stripe",
    "name": "Stripe",
    "vendor": "Stripe",
    "logo": "stripe",
    "category": "ERP & finance",
    "industries": [
      "Financial services",
      "Retail"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Payments",
      "Customers",
      "Subscriptions"
    ],
    "docs": "https://docs.stripe.com/api",
    "status": "planned"
  },
  {
    "id": "shopify",
    "name": "Shopify",
    "vendor": "Shopify",
    "logo": "shopify",
    "category": "Commerce & logistics",
    "industries": [
      "Retail"
    ],
    "protocols": [
      "GraphQL"
    ],
    "areas": [
      "Products",
      "Orders",
      "Fulfillment"
    ],
    "docs": "https://shopify.dev/docs/api",
    "status": "planned"
  },
  {
    "id": "amadeus",
    "name": "Amadeus",
    "vendor": "Amadeus",
    "logo": null,
    "category": "Commerce & logistics",
    "industries": [
      "Travel",
      "Logistics"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Flight offers",
      "Bookings",
      "Airports"
    ],
    "docs": "https://developers.amadeus.com/",
    "status": "planned"
  },
  {
    "id": "sabre",
    "name": "Sabre",
    "vendor": "Sabre",
    "logo": null,
    "category": "Commerce & logistics",
    "industries": [
      "Travel"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Shopping",
      "Reservations",
      "Ticketing"
    ],
    "docs": "https://developer.sabre.com/",
    "status": "planned"
  },
  {
    "id": "epic-fhir",
    "name": "Epic FHIR",
    "vendor": "Epic",
    "logo": null,
    "category": "Healthcare",
    "industries": [
      "Healthcare"
    ],
    "protocols": [
      "FHIR API"
    ],
    "areas": [
      "Patients",
      "Encounters",
      "Observations"
    ],
    "docs": "https://fhir.epic.com/",
    "status": "planned"
  },
  {
    "id": "postgresql",
    "name": "PostgreSQL",
    "vendor": "PostgreSQL",
    "logo": "postgresql",
    "category": "Data & infrastructure",
    "industries": [
      "Technology",
      "Financial services",
      "Healthcare"
    ],
    "protocols": [
      "SQL"
    ],
    "areas": [
      "Tables",
      "Queries",
      "Transactions"
    ],
    "docs": "https://www.postgresql.org/docs/current/",
    "status": "planned"
  },
  {
    "id": "snowflake",
    "name": "Snowflake",
    "vendor": "Snowflake",
    "logo": "snowflake",
    "category": "Data & infrastructure",
    "industries": [
      "Retail",
      "Financial services",
      "Healthcare"
    ],
    "protocols": [
      "SQL",
      "REST API"
    ],
    "areas": [
      "Queries",
      "Warehouses",
      "Data"
    ],
    "docs": "https://docs.snowflake.com/en/developer-guide/sql-api/index",
    "status": "planned"
  },
  {
    "id": "aws",
    "name": "Amazon Web Services",
    "vendor": "Amazon",
    "logo": "amazonwebservices",
    "category": "Data & infrastructure",
    "industries": [
      "Technology",
      "Financial services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Compute",
      "Storage",
      "Queues"
    ],
    "docs": "https://docs.aws.amazon.com/",
    "status": "planned"
  },
  {
    "id": "datadog",
    "name": "Datadog",
    "vendor": "Datadog",
    "logo": "datadog",
    "category": "Observability",
    "industries": [
      "Technology",
      "Financial services"
    ],
    "protocols": [
      "REST API",
      "OTLP"
    ],
    "areas": [
      "Metrics",
      "Monitors",
      "Logs"
    ],
    "docs": "https://docs.datadoghq.com/api/latest/",
    "status": "planned"
  },
  {
    "id": "grafana",
    "name": "Grafana",
    "vendor": "Grafana Labs",
    "logo": "grafana",
    "category": "Observability",
    "industries": [
      "Technology",
      "Manufacturing"
    ],
    "protocols": [
      "REST API",
      "OTLP"
    ],
    "areas": [
      "Dashboards",
      "Alerts",
      "Traces"
    ],
    "docs": "https://grafana.com/docs/",
    "status": "planned"
  },
  {
    "id": "pagerduty",
    "name": "PagerDuty",
    "vendor": "PagerDuty",
    "logo": "pagerduty",
    "category": "Observability",
    "industries": [
      "Technology",
      "Healthcare",
      "Financial services"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Incidents",
      "On-call schedules",
      "Escalations"
    ],
    "docs": "https://developer.pagerduty.com/",
    "status": "planned"
  },
  {
    "id": "sentry",
    "name": "Sentry",
    "vendor": "Sentry",
    "logo": "sentry",
    "category": "Observability",
    "industries": [
      "Technology",
      "Retail"
    ],
    "protocols": [
      "REST API"
    ],
    "areas": [
      "Issues",
      "Projects",
      "Releases"
    ],
    "docs": "https://docs.sentry.io/api/",
    "status": "planned"
  }
];
