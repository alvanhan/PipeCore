-- PostgreSQL fixture DDL. Apply within POSTGRES_DB=pipecore_test.
CREATE TABLE IF NOT EXISTS public.finance_transactions (
    transaction_id VARCHAR(40) PRIMARY KEY,
    account_id VARCHAR(36) NOT NULL,
    customer_id VARCHAR(40) NOT NULL,
    reference_no VARCHAR(50),
    posting_date DATE NOT NULL,
    event_time TIMESTAMP(6) NOT NULL,
    settled_at TIMESTAMPTZ(6) NOT NULL,
    time_of_day TIME(6) NOT NULL,
    currency_code VARCHAR(3) NOT NULL,
    debit_amount NUMERIC(20,4) NOT NULL,
    credit_amount NUMERIC(20,4) NOT NULL,
    balance_after NUMERIC(24,4) NOT NULL,
    fx_rate NUMERIC(20,8) NOT NULL,
    tax_amount NUMERIC(20,4) NOT NULL,
    quantity BIGINT NOT NULL,
    units INTEGER NOT NULL,
    is_reconciled BOOLEAN NOT NULL,
    branch_code VARCHAR(10) NOT NULL,
    description VARCHAR(200),
    source_system VARCHAR(30) NOT NULL,
    created_at TIMESTAMP(6) NOT NULL,
    batch_no INTEGER NOT NULL,
    risk_score DOUBLE PRECISION NOT NULL,
    external_uuid UUID NOT NULL,
    smallint_code SMALLINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_finance_transactions_posting_date ON public.finance_transactions(posting_date);
CREATE TABLE IF NOT EXISTS public.finance_accounts (
    account_id VARCHAR(36) PRIMARY KEY,
    account_name VARCHAR(120) NOT NULL,
    status VARCHAR(20) NOT NULL,
    opened_date DATE NOT NULL,
    current_balance NUMERIC(24,4) NOT NULL
);
