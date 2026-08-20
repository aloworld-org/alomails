-- Preserve the pre-ledger campaign delivery tables that some long-lived local
-- workspaces already carry. They predate SQLx migration 0800 and use the same
-- `campaign_sends` name for a different row shape (one row per address rather
-- than one row per act of sending).
--
-- Migration 0800 cannot safely reinterpret those rows, and dropping them would
-- erase delivery history. Rename the old tables and their schema-global indexes
-- instead. Fresh databases take the no-op branch and receive 0800 unchanged.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'campaign_sends'
          AND column_name = 'address'
    ) AND NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'campaign_sends'
          AND column_name = 'topic_fold'
    ) THEN
        ALTER TABLE campaign_sends RENAME TO campaign_delivery_sends_legacy;

        IF to_regclass('public.campaign_send_events') IS NOT NULL THEN
            ALTER TABLE campaign_send_events
                RENAME TO campaign_delivery_send_events_legacy;
        END IF;

        IF to_regclass('public.campaign_sends_pkey') IS NOT NULL THEN
            ALTER INDEX campaign_sends_pkey
                RENAME TO campaign_delivery_sends_legacy_pkey;
        END IF;

        IF to_regclass('public.campaign_sends_by_campaign') IS NOT NULL THEN
            ALTER INDEX campaign_sends_by_campaign
                RENAME TO campaign_delivery_sends_legacy_by_campaign;
        END IF;

        IF to_regclass('public.campaign_sends_by_address') IS NOT NULL THEN
            ALTER INDEX campaign_sends_by_address
                RENAME TO campaign_delivery_sends_legacy_by_address;
        END IF;

        IF to_regclass('public.campaign_sends_one_per_person') IS NOT NULL THEN
            ALTER INDEX campaign_sends_one_per_person
                RENAME TO campaign_delivery_sends_legacy_one_per_person;
        END IF;

        IF to_regclass('public.campaign_send_events_pkey') IS NOT NULL THEN
            ALTER INDEX campaign_send_events_pkey
                RENAME TO campaign_delivery_send_events_legacy_pkey;
        END IF;

        IF to_regclass('public.campaign_send_events_by_send') IS NOT NULL THEN
            ALTER INDEX campaign_send_events_by_send
                RENAME TO campaign_delivery_send_events_legacy_by_send;
        END IF;
    END IF;
END $$;
