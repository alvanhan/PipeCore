-- SQL Server 2022 test fixture. Execute with sqlcmd (GO batch separators).
IF DB_ID(N'PipeCoreTest') IS NULL CREATE DATABASE [PipeCoreTest];
GO
USE [PipeCoreTest];
GO
IF OBJECT_ID(N'dbo.finance_transactions', N'U') IS NULL
BEGIN
    CREATE TABLE dbo.finance_transactions (
        transaction_id NVARCHAR(40) NOT NULL PRIMARY KEY,
        account_id NVARCHAR(36) NOT NULL,
        customer_id NVARCHAR(40) NOT NULL,
        reference_no NVARCHAR(50) NULL,
        posting_date DATE NOT NULL,
        event_time DATETIME2(6) NOT NULL,
        settled_at DATETIMEOFFSET(6) NOT NULL,
        time_of_day TIME(6) NOT NULL,
        currency_code NVARCHAR(3) NOT NULL,
        debit_amount DECIMAL(20,4) NOT NULL,
        credit_amount DECIMAL(20,4) NOT NULL,
        balance_after DECIMAL(24,4) NOT NULL,
        fx_rate DECIMAL(20,8) NOT NULL,
        tax_amount DECIMAL(20,4) NOT NULL,
        quantity BIGINT NOT NULL,
        units INT NOT NULL,
        is_reconciled BIT NOT NULL,
        branch_code NVARCHAR(10) NOT NULL,
        description NVARCHAR(200) NULL,
        source_system NVARCHAR(30) NOT NULL,
        created_at DATETIME2(6) NOT NULL,
        batch_no INT NOT NULL,
        risk_score FLOAT(53) NOT NULL,
        external_uuid UNIQUEIDENTIFIER NOT NULL,
        smallint_code SMALLINT NOT NULL
    );
END;
GO
IF NOT EXISTS(SELECT 1 FROM sys.indexes WHERE name = 'idx_finance_transactions_posting_date' AND object_id = OBJECT_ID('dbo.finance_transactions'))
    CREATE INDEX idx_finance_transactions_posting_date ON dbo.finance_transactions(posting_date);
GO
IF OBJECT_ID(N'dbo.finance_accounts', N'U') IS NULL
BEGIN
    CREATE TABLE dbo.finance_accounts (
        account_id NVARCHAR(36) NOT NULL PRIMARY KEY,
        account_name NVARCHAR(120) NOT NULL,
        status NVARCHAR(20) NOT NULL,
        opened_date DATE NOT NULL,
        current_balance DECIMAL(24,4) NOT NULL
    );
END;
GO
