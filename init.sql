--
-- PostgreSQL database dump
-- $ pg_dump postgres -sOcn lean4oj -f init.sql
--

-- Dumped from database version 18.0
-- Dumped by pg_dump version 18.0

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

ALTER TABLE ONLY lean4oj.user_preference DROP CONSTRAINT user_preference_uid_fkey;
ALTER TABLE ONLY lean4oj.user_information DROP CONSTRAINT user_information_uid_fkey;
ALTER TABLE ONLY lean4oj.user_groups DROP CONSTRAINT user_groups_uid_fkey;
ALTER TABLE ONLY lean4oj.user_groups DROP CONSTRAINT user_groups_gid_fkey;
ALTER TABLE ONLY lean4oj.problems DROP CONSTRAINT problems_owner_fkey;
ALTER TABLE ONLY lean4oj.discussions DROP CONSTRAINT discussions_publisher_fkey;
ALTER TABLE ONLY lean4oj.discussion_replies DROP CONSTRAINT discussion_replies_publisher_fkey;
ALTER TABLE ONLY lean4oj.discussion_replies DROP CONSTRAINT discussion_replies_did_fkey;
ALTER TABLE ONLY lean4oj.discussion_reactions DROP CONSTRAINT discussion_reactions_uid_fkey;
DROP INDEX lean4oj.users_ac_idx;
DROP INDEX lean4oj.user_groups_gid_uid_idx;
DROP INDEX lean4oj.discussion_replies_did_id_idx;
DROP INDEX lean4oj.discussion_reactions_eid_emoji_idx;
ALTER TABLE ONLY lean4oj.users DROP CONSTRAINT users_pkey;
ALTER TABLE ONLY lean4oj.users DROP CONSTRAINT users_email_key;
ALTER TABLE ONLY lean4oj.user_preference DROP CONSTRAINT user_preference_pkey;
ALTER TABLE ONLY lean4oj.user_information DROP CONSTRAINT user_information_pkey;
ALTER TABLE ONLY lean4oj.user_groups DROP CONSTRAINT user_groups_pkey;
ALTER TABLE ONLY lean4oj.tags DROP CONSTRAINT tags_pkey;
ALTER TABLE ONLY lean4oj.problems DROP CONSTRAINT problems_pkey;
ALTER TABLE ONLY lean4oj.groups DROP CONSTRAINT groups_pkey;
ALTER TABLE ONLY lean4oj.discussions DROP CONSTRAINT discussions_pkey;
ALTER TABLE ONLY lean4oj.discussion_replies DROP CONSTRAINT discussion_replies_pkey;
ALTER TABLE ONLY lean4oj.discussion_reactions DROP CONSTRAINT discussion_reactions_pkey;
ALTER TABLE lean4oj.tags ALTER COLUMN id DROP DEFAULT;
ALTER TABLE lean4oj.problems ALTER COLUMN pid DROP DEFAULT;
ALTER TABLE lean4oj.discussions ALTER COLUMN id DROP DEFAULT;
ALTER TABLE lean4oj.discussion_replies ALTER COLUMN id DROP DEFAULT;
DROP TABLE lean4oj.users;
DROP TABLE lean4oj.user_preference;
DROP TABLE lean4oj.user_information;
DROP TABLE lean4oj.user_groups;
DROP SEQUENCE lean4oj.tags_id_seq;
DROP TABLE lean4oj.tags;
DROP SEQUENCE lean4oj.problems_pid_seq;
DROP TABLE lean4oj.problems;
DROP TABLE lean4oj.groups;
DROP SEQUENCE lean4oj.discussions_id_seq;
DROP TABLE lean4oj.discussions;
DROP SEQUENCE lean4oj.discussion_replies_id_seq;
DROP TABLE lean4oj.discussion_replies;
DROP TABLE lean4oj.discussion_reactions;
DROP SCHEMA lean4oj;
--
-- Name: lean4oj; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA lean4oj;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: discussion_reactions; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.discussion_reactions (
    eid integer NOT NULL,
    uid character varying(24) NOT NULL COLLATE public.case_insensitive,
    emoji character varying(8) NOT NULL
);


--
-- Name: discussion_replies; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.discussion_replies (
    id integer NOT NULL,
    content text NOT NULL,
    publish timestamp without time zone NOT NULL,
    edit timestamp without time zone NOT NULL,
    did integer NOT NULL,
    publisher character varying(24) NOT NULL COLLATE public.case_insensitive
);


--
-- Name: discussion_replies_id_seq; Type: SEQUENCE; Schema: lean4oj; Owner: -
--

CREATE SEQUENCE lean4oj.discussion_replies_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: discussion_replies_id_seq; Type: SEQUENCE OWNED BY; Schema: lean4oj; Owner: -
--

ALTER SEQUENCE lean4oj.discussion_replies_id_seq OWNED BY lean4oj.discussion_replies.id;


--
-- Name: discussions; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.discussions (
    id integer NOT NULL,
    title character varying(80) NOT NULL,
    content text NOT NULL,
    publish timestamp without time zone NOT NULL,
    edit timestamp without time zone NOT NULL,
    update timestamp without time zone CONSTRAINT discussions_reply_latest_not_null NOT NULL,
    reply_count integer DEFAULT 0 CONSTRAINT discussions_reply_count1_not_null NOT NULL,
    publisher character varying(24) CONSTRAINT discussions_publisher1_not_null NOT NULL COLLATE public.case_insensitive
);


--
-- Name: discussions_id_seq; Type: SEQUENCE; Schema: lean4oj; Owner: -
--

CREATE SEQUENCE lean4oj.discussions_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: discussions_id_seq; Type: SEQUENCE OWNED BY; Schema: lean4oj; Owner: -
--

ALTER SEQUENCE lean4oj.discussions_id_seq OWNED BY lean4oj.discussions.id;


--
-- Name: groups; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.groups (
    gid character varying(48) NOT NULL COLLATE public.case_insensitive,
    member_count integer NOT NULL
);


--
-- Name: problems; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.problems (
    pid integer NOT NULL,
    is_public boolean DEFAULT false NOT NULL,
    public_at timestamp without time zone DEFAULT '1970-01-01 00:00:00'::timestamp without time zone NOT NULL,
    owner character varying(24) NOT NULL COLLATE public.case_insensitive,
    content jsonb NOT NULL,
    sub integer DEFAULT 0 NOT NULL,
    ac integer DEFAULT 0 NOT NULL,
    jb jsonb NOT NULL
);


--
-- Name: problems_pid_seq; Type: SEQUENCE; Schema: lean4oj; Owner: -
--

CREATE SEQUENCE lean4oj.problems_pid_seq
    AS integer
    START WITH -1
    INCREMENT BY -1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: problems_pid_seq; Type: SEQUENCE OWNED BY; Schema: lean4oj; Owner: -
--

ALTER SEQUENCE lean4oj.problems_pid_seq OWNED BY lean4oj.problems.pid;


--
-- Name: tags; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.tags (
    id integer NOT NULL,
    color character varying(24) NOT NULL,
    name jsonb NOT NULL
);


--
-- Name: tags_id_seq; Type: SEQUENCE; Schema: lean4oj; Owner: -
--

CREATE SEQUENCE lean4oj.tags_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: tags_id_seq; Type: SEQUENCE OWNED BY; Schema: lean4oj; Owner: -
--

ALTER SEQUENCE lean4oj.tags_id_seq OWNED BY lean4oj.tags.id;


--
-- Name: user_groups; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.user_groups (
    uid character varying(24) NOT NULL COLLATE public.case_insensitive,
    gid character varying(48) NOT NULL COLLATE public.case_insensitive,
    is_admin boolean DEFAULT false NOT NULL
);


--
-- Name: user_information; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.user_information (
    uid character varying(24) NOT NULL COLLATE public.case_insensitive,
    organization character varying(80) DEFAULT ''::character varying NOT NULL,
    location character varying(80) DEFAULT ''::character varying NOT NULL,
    url character varying(80) DEFAULT ''::character varying NOT NULL,
    telegram character varying(30) DEFAULT ''::character varying NOT NULL,
    qq character varying(30) DEFAULT ''::character varying NOT NULL,
    github character varying(30) DEFAULT ''::character varying NOT NULL
);


--
-- Name: user_preference; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.user_preference (
    uid character varying(24) NOT NULL COLLATE public.case_insensitive,
    preference jsonb DEFAULT '{}'::jsonb NOT NULL
);


--
-- Name: users; Type: TABLE; Schema: lean4oj; Owner: -
--

CREATE TABLE lean4oj.users (
    uid character varying(24) NOT NULL COLLATE public.case_insensitive,
    username character varying(24) NOT NULL,
    email character varying(256) NOT NULL COLLATE public.case_insensitive,
    password character varying(43) NOT NULL,
    register_time timestamp without time zone NOT NULL,
    ac integer DEFAULT 0 NOT NULL,
    nickname character varying(24) DEFAULT ''::character varying NOT NULL,
    bio character varying(160) DEFAULT ''::character varying NOT NULL,
    avatar_info character varying(272) NOT NULL
);


--
-- Name: discussion_replies id; Type: DEFAULT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_replies ALTER COLUMN id SET DEFAULT nextval('lean4oj.discussion_replies_id_seq'::regclass);


--
-- Name: discussions id; Type: DEFAULT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussions ALTER COLUMN id SET DEFAULT nextval('lean4oj.discussions_id_seq'::regclass);


--
-- Name: problems pid; Type: DEFAULT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.problems ALTER COLUMN pid SET DEFAULT nextval('lean4oj.problems_pid_seq'::regclass);


--
-- Name: tags id; Type: DEFAULT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.tags ALTER COLUMN id SET DEFAULT nextval('lean4oj.tags_id_seq'::regclass);





--
-- Data for Name: user_groups; Type: TABLE DATA; Schema: lean4oj; Owner: -
--

COPY lean4oj.groups (gid, member_count) FROM stdin;
Lean4OJ.Admin	0
Lean4OJ.EditHomepage	0
Lean4OJ.Judger	0
Lean4OJ.ManageContest	0
Lean4OJ.ManageDiscussion	0
Lean4OJ.ManageProblem	0
Lean4OJ.ManageUser	0
Lean4OJ.ManageUserGroup	0
Lean4OJ.TooManyOLeans	0
\.

--
-- Data for Name: users; Type: TABLE DATA; Schema: lean4oj; Owner: -
--

COPY lean4oj.users (uid, username, email, password, register_time, ac, nickname, bio, avatar_info) FROM stdin;
Aesop		Aesop		1970-01-01 00:00:00	0			
Archive		Archive		1970-01-01 00:00:00	0			
Batteries		Batteries		1970-01-01 00:00:00	0			
Counterexamples		Counterexamples		1970-01-01 00:00:00	0			
ImportGraph		ImportGraph		1970-01-01 00:00:00	0			
Init		Init		1970-01-01 00:00:00	0			
Lake		Lake		1970-01-01 00:00:00	0			
Lean		Lean		1970-01-01 00:00:00	0			
LeanSearchClient		LeanSearchClient		1970-01-01 00:00:00	0			
Mathlib		Mathlib		1970-01-01 00:00:00	0			
Plausible		Plausible		1970-01-01 00:00:00	0			
ProofWidgets		ProofWidgets		1970-01-01 00:00:00	0			
Std		Std		1970-01-01 00:00:00	0			
docs		docs		1970-01-01 00:00:00	0			
references		references		1970-01-01 00:00:00	0			
Lean4OJ		Lean4OJ		1970-01-01 00:00:00	0			
build		build		1970-01-01 00:00:00	0			
\.

--
-- Name: discussion_reactions discussion_reactions_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_reactions
    ADD CONSTRAINT discussion_reactions_pkey PRIMARY KEY (eid, uid, emoji);


--
-- Name: discussion_replies discussion_replies_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_replies
    ADD CONSTRAINT discussion_replies_pkey PRIMARY KEY (id);


--
-- Name: discussions discussions_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussions
    ADD CONSTRAINT discussions_pkey PRIMARY KEY (id);


--
-- Name: groups groups_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (gid);


--
-- Name: problems problems_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.problems
    ADD CONSTRAINT problems_pkey PRIMARY KEY (pid);


--
-- Name: tags tags_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.tags
    ADD CONSTRAINT tags_pkey PRIMARY KEY (id);


--
-- Name: user_groups user_groups_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_groups
    ADD CONSTRAINT user_groups_pkey PRIMARY KEY (uid, gid);


--
-- Name: user_information user_information_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_information
    ADD CONSTRAINT user_information_pkey PRIMARY KEY (uid);


--
-- Name: user_preference user_preference_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_preference
    ADD CONSTRAINT user_preference_pkey PRIMARY KEY (uid);


--
-- Name: users users_email_key; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (uid);


--
-- Name: discussion_reactions_eid_emoji_idx; Type: INDEX; Schema: lean4oj; Owner: -
--

CREATE INDEX discussion_reactions_eid_emoji_idx ON lean4oj.discussion_reactions USING btree (eid, emoji);


--
-- Name: discussion_replies_did_id_idx; Type: INDEX; Schema: lean4oj; Owner: -
--

CREATE INDEX discussion_replies_did_id_idx ON lean4oj.discussion_replies USING btree (did, id);


--
-- Name: user_groups_gid_uid_idx; Type: INDEX; Schema: lean4oj; Owner: -
--

CREATE INDEX user_groups_gid_uid_idx ON lean4oj.user_groups USING btree (gid, uid);


--
-- Name: users_ac_idx; Type: INDEX; Schema: lean4oj; Owner: -
--

CREATE INDEX users_ac_idx ON lean4oj.users USING btree (ac);


--
-- Name: discussion_reactions discussion_reactions_uid_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_reactions
    ADD CONSTRAINT discussion_reactions_uid_fkey FOREIGN KEY (uid) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- Name: discussion_replies discussion_replies_did_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_replies
    ADD CONSTRAINT discussion_replies_did_fkey FOREIGN KEY (did) REFERENCES lean4oj.discussions(id) MATCH FULL ON DELETE CASCADE;


--
-- Name: discussion_replies discussion_replies_publisher_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussion_replies
    ADD CONSTRAINT discussion_replies_publisher_fkey FOREIGN KEY (publisher) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- Name: discussions discussions_publisher_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.discussions
    ADD CONSTRAINT discussions_publisher_fkey FOREIGN KEY (publisher) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- Name: problems problems_owner_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.problems
    ADD CONSTRAINT problems_owner_fkey FOREIGN KEY (owner) REFERENCES lean4oj.users(uid);


--
-- Name: user_groups user_groups_gid_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_groups
    ADD CONSTRAINT user_groups_gid_fkey FOREIGN KEY (gid) REFERENCES lean4oj.groups(gid) MATCH FULL;


--
-- Name: user_groups user_groups_uid_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_groups
    ADD CONSTRAINT user_groups_uid_fkey FOREIGN KEY (uid) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- Name: user_information user_information_uid_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_information
    ADD CONSTRAINT user_information_uid_fkey FOREIGN KEY (uid) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- Name: user_preference user_preference_uid_fkey; Type: FK CONSTRAINT; Schema: lean4oj; Owner: -
--

ALTER TABLE ONLY lean4oj.user_preference
    ADD CONSTRAINT user_preference_uid_fkey FOREIGN KEY (uid) REFERENCES lean4oj.users(uid) MATCH FULL;


--
-- PostgreSQL database dump complete
--

