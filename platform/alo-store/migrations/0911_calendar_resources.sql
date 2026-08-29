-- Rooms and resources (Agenda launch tier: "rooms and resources you can
-- actually book"). A resource — a meeting room, a car, a projector — is a
-- calendar of kind `resource` in the tenant's name, plus the facts that make
-- it bookable: an address to name it by, where it is, and how many people it
-- seats.
--
-- Why a `calendars` row rather than a table of its own: a resource has a
-- schedule, and Agenda already knows exactly one way to hold one. Reusing the
-- row gives the resource a stable id that is also its CalDAV collection
-- segment, and gives the *refusal* to edit it for free — `editable_pred`
-- excludes kind = 'resource', so no owner and no grant can ever write into a
-- room's calendar directly. Bookings arrive one way only: through an event
-- that names the room.
CREATE TABLE calendar_resources (
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- The `calendars` row this resource *is* (kind = 'resource'); its name and
    -- colour live there, so a room is listed and rendered like any calendar.
    calendar_id TEXT NOT NULL,
    -- How an event names the room: the address that appears as an ATTENDEE.
    -- Unique per tenant, case-insensitively, and never a person's address —
    -- one string, one thing it can mean.
    email       TEXT NOT NULL,
    -- Where the room is ("2nd floor, east wing"), for the picker.
    location    TEXT,
    -- How many people it seats; NULL when nobody said.
    capacity    INTEGER CHECK (capacity IS NULL OR (capacity > 0 AND capacity <= 100000)),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, calendar_id),
    FOREIGN KEY (tenant_id, calendar_id)
        REFERENCES calendars (tenant_id, id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX calendar_resources_by_email
    ON calendar_resources (tenant_id, lower(email));

-- Which events hold which resource. The link carries no times on purpose:
-- *when* a room is taken is always read from the event itself, so moving a
-- meeting moves its room booking with it and the two can never disagree.
--
-- No foreign key to calendar_events: that table is keyed by (tenant, user,
-- id) and a booking is not the booker's to key by. Every read joins the event
-- instead, so a link whose event is gone occupies nothing.
CREATE TABLE calendar_resource_bookings (
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL,
    event_id    TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, resource_id, event_id),
    FOREIGN KEY (tenant_id, resource_id)
        REFERENCES calendar_resources (tenant_id, calendar_id) ON DELETE CASCADE
);
-- "Which rooms does this event hold?" — the reconcile path on every save.
CREATE INDEX calendar_resource_bookings_by_event
    ON calendar_resource_bookings (tenant_id, event_id);
